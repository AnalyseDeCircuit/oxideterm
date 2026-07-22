use std::{
    collections::{BTreeMap, HashMap},
    env, fmt,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use agent_client_protocol::{
    AcpAgent, ActiveSession, Agent, Client, ConnectTo, ConnectionTo, Lines, Role, SessionMessage,
    schema::{
        AgentCapabilities, AuthMethod, AuthMethodId, AuthenticateRequest, AuthenticateResponse,
        CancelNotification, ClientCapabilities, CloseSessionRequest, CloseSessionResponse,
        CreateTerminalRequest, CreateTerminalResponse, DeleteSessionRequest, DeleteSessionResponse,
        EnvVariable, FileSystemCapabilities, Implementation, InitializeRequest, InitializeResponse,
        KillTerminalRequest, KillTerminalResponse, ListSessionsRequest, ListSessionsResponse,
        LoadSessionRequest, LoadSessionResponse, LogoutRequest, LogoutResponse, McpServer,
        McpServerStdio, NewSessionRequest, NewSessionResponse, PermissionOptionKind,
        ProtocolVersion, ReadTextFileRequest, ReadTextFileResponse, ReleaseTerminalRequest,
        ReleaseTerminalResponse, RequestPermissionOutcome, RequestPermissionRequest,
        RequestPermissionResponse, ResumeSessionRequest, ResumeSessionResponse,
        SelectedPermissionOutcome, SessionConfigId, SessionConfigKind, SessionConfigOption,
        SessionConfigOptionCategory, SessionConfigSelectOptions, SessionConfigValueId, SessionId,
        SessionModeId, SessionNotification, SessionUpdate, SetSessionConfigOptionRequest,
        SetSessionConfigOptionResponse, SetSessionModeRequest, SetSessionModeResponse,
        TerminalExitStatus, TerminalOutputRequest, TerminalOutputResponse, ToolCall,
        ToolCallStatus, ToolCallUpdate, WaitForTerminalExitRequest, WaitForTerminalExitResponse,
        WriteTextFileRequest, WriteTextFileResponse,
    },
    util::MatchDispatch,
};
use futures::{AsyncBufReadExt, AsyncWriteExt, FutureExt, StreamExt, pin_mut};
use thiserror::Error;
use tokio::{
    io::AsyncReadExt as _,
    sync::{Mutex, mpsc, oneshot, watch},
};

#[cfg(windows)]
use async_process::windows::CommandExt as AsyncProcessCommandExt;

use crate::types::AiStreamEvent;

#[cfg(windows)]
const ACP_BACKGROUND_PROCESS_CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Clone, PartialEq)]
pub struct AcpLaunchConfig {
    pub id: String,
    pub display_name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub cwd: Option<PathBuf>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AcpHostCapabilityPolicy {
    pub fs_read_text_file: bool,
    pub fs_write_text_file: bool,
    pub terminal: bool,
}

#[derive(Debug)]
pub struct AcpStdioLauncher {
    config: AcpLaunchConfig,
}

#[derive(Debug)]
pub struct AcpAgentRuntime {
    connection: ConnectionTo<Agent>,
    initialize_response: InitializeResponse,
}

#[derive(Debug)]
pub struct AcpActiveSession {
    inner: ActiveSession<'static, Agent>,
    config_options: Vec<SessionConfigOption>,
}

pub type AcpClientEventSender = mpsc::UnboundedSender<AcpClientEvent>;
type AcpClientResponseSender<T> = oneshot::Sender<Result<T, agent_client_protocol::Error>>;
static ACP_TERMINAL_COUNTER: AtomicU64 = AtomicU64::new(1);

pub enum AcpClientEvent {
    SessionUpdate(SessionNotification),
    RequestPermission {
        request: RequestPermissionRequest,
        response_tx: AcpClientResponseSender<RequestPermissionResponse>,
    },
    ReadTextFile {
        request: ReadTextFileRequest,
        response_tx: AcpClientResponseSender<ReadTextFileResponse>,
    },
    WriteTextFile {
        request: WriteTextFileRequest,
        response_tx: AcpClientResponseSender<WriteTextFileResponse>,
    },
    CreateTerminal {
        request: CreateTerminalRequest,
        response_tx: AcpClientResponseSender<CreateTerminalResponse>,
    },
    TerminalOutput {
        request: TerminalOutputRequest,
        response_tx: AcpClientResponseSender<TerminalOutputResponse>,
    },
    ReleaseTerminal {
        request: ReleaseTerminalRequest,
        response_tx: AcpClientResponseSender<ReleaseTerminalResponse>,
    },
    WaitForTerminalExit {
        request: WaitForTerminalExitRequest,
        response_tx: AcpClientResponseSender<WaitForTerminalExitResponse>,
    },
    KillTerminal {
        request: KillTerminalRequest,
        response_tx: AcpClientResponseSender<KillTerminalResponse>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcpPromptSessionOutcome {
    pub session_id: String,
    pub session_metadata: Option<serde_json::Value>,
    pub session_config_options: Vec<AcpSessionConfigOption>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpSessionConfigChoice {
    pub value_id: String,
    pub label: String,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpSessionConfigOption {
    pub config_id: String,
    pub name: String,
    pub category: Option<String>,
    pub current_value_id: String,
    pub choices: Vec<AcpSessionConfigChoice>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpSessionConfigSelection {
    pub config_id: String,
    pub value_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AcpLaunchModelHint {
    Automatic,
    Fixed(String),
}

/// Projects protocol configuration into stable, serializable application state.
pub fn acp_session_config_options(options: &[SessionConfigOption]) -> Vec<AcpSessionConfigOption> {
    options
        .iter()
        .filter_map(|option| {
            let SessionConfigKind::Select(select) = &option.kind else {
                return None;
            };
            let choices = match &select.options {
                SessionConfigSelectOptions::Ungrouped(options) => options
                    .iter()
                    .map(|choice| AcpSessionConfigChoice {
                        value_id: choice.value.to_string(),
                        label: choice.name.clone(),
                    })
                    .collect(),
                SessionConfigSelectOptions::Grouped(groups) => groups
                    .iter()
                    .flat_map(|group| group.options.iter())
                    .map(|choice| AcpSessionConfigChoice {
                        value_id: choice.value.to_string(),
                        label: choice.name.clone(),
                    })
                    .collect(),
                _ => Vec::new(),
            };
            let category = option.category.as_ref().map(|category| match category {
                SessionConfigOptionCategory::Mode => "mode".to_string(),
                SessionConfigOptionCategory::Model => "model".to_string(),
                SessionConfigOptionCategory::ThoughtLevel => "thought_level".to_string(),
                SessionConfigOptionCategory::Other(category) => category.clone(),
                _ => "unknown".to_string(),
            });
            Some(AcpSessionConfigOption {
                config_id: option.id.to_string(),
                name: option.name.clone(),
                category,
                current_value_id: select.current_value.to_string(),
                choices,
            })
        })
        .collect()
}

/// Returns the first model selector because ACP defines option order as priority.
pub fn acp_model_config_option(
    options: &[AcpSessionConfigOption],
) -> Option<&AcpSessionConfigOption> {
    options.iter().find(|option| {
        option.category.as_deref() == Some("model")
            || (option.category.is_none()
                && (option.config_id.eq_ignore_ascii_case("model")
                    || option.name.eq_ignore_ascii_case("model")))
    })
}

/// Reads an explicit model flag without assuming that an ACP agent exposes model metadata.
pub fn acp_launch_model_hint(args: &[String]) -> Option<AcpLaunchModelHint> {
    let mut args = args.iter();
    while let Some(argument) = args.next() {
        let value = if argument == "--model" {
            args.next().map(String::as_str)
        } else {
            argument.strip_prefix("--model=")
        };
        let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
            continue;
        };
        return Some(if value.eq_ignore_ascii_case("auto") {
            AcpLaunchModelHint::Automatic
        } else {
            AcpLaunchModelHint::Fixed(value.to_string())
        });
    }
    None
}

/// Detects OxideTerm's native Claude Code adapter, which learns the resolved model from the
/// first prompt's stream instead of during ACP session creation.
pub fn acp_model_report_is_deferred_until_first_prompt(args: &[String]) -> bool {
    args.windows(2).any(|arguments| {
        arguments[0] == "--acp-adapter" && arguments[1].eq_ignore_ascii_case("claude-code")
    })
}

/// Detects OxideTerm's native Codex adapter, which publishes its model choices during
/// ACP session creation without requiring a user prompt.
pub fn acp_model_report_is_available_during_session_start(args: &[String]) -> bool {
    args.windows(2).any(|arguments| {
        arguments[0] == "--acp-adapter" && arguments[1].eq_ignore_ascii_case("codex")
    })
}

/// Resolves a stored choice only while it remains valid in the latest snapshot.
pub fn acp_selected_config_choice<'a>(
    option: &'a AcpSessionConfigOption,
    selection: Option<&AcpSessionConfigSelection>,
) -> Option<&'a AcpSessionConfigChoice> {
    let selected_value = selection
        .filter(|selection| selection.config_id == option.config_id)
        .and_then(|selection| {
            option
                .choices
                .iter()
                .find(|choice| choice.value_id == selection.value_id)
        });
    selected_value.or_else(|| {
        option
            .choices
            .iter()
            .find(|choice| choice.value_id == option.current_value_id)
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcpPermissionOptionProjection {
    pub option_id: String,
    pub name: String,
    pub kind: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcpPermissionRequestProjection {
    pub tool_call_id: String,
    pub name: String,
    pub arguments: String,
    pub summary: String,
    pub risk: String,
    pub options: Vec<AcpPermissionOptionProjection>,
}

pub fn acp_session_notification_to_ai_stream_events(
    notification: &SessionNotification,
) -> Vec<AiStreamEvent> {
    match &notification.update {
        SessionUpdate::AgentMessageChunk(chunk) => text_content(&chunk.content)
            .map(|text| vec![AiStreamEvent::Content(text.to_string())])
            .unwrap_or_default(),
        SessionUpdate::AgentThoughtChunk(chunk) => text_content(&chunk.content)
            .map(|text| vec![AiStreamEvent::Thinking(text.to_string())])
            .unwrap_or_default(),
        SessionUpdate::ToolCall(tool_call) => {
            vec![acp_tool_call_stream_event(tool_call, false)]
        }
        SessionUpdate::ToolCallUpdate(update) => {
            vec![acp_tool_call_update_stream_event(update)]
        }
        _ => Vec::new(),
    }
}

pub fn acp_permission_request_projection(
    request: &RequestPermissionRequest,
) -> AcpPermissionRequestProjection {
    let tool_call_id = request.tool_call.tool_call_id.to_string();
    let name = request
        .tool_call
        .fields
        .title
        .clone()
        .unwrap_or_else(|| tool_call_id.clone());
    let arguments = acp_tool_arguments(
        request.tool_call.fields.raw_input.as_ref(),
        request.tool_call.fields.raw_output.as_ref(),
        request.tool_call.fields.status,
        request.tool_call.fields.content.as_ref(),
    );
    let options = request
        .options
        .iter()
        .map(|option| AcpPermissionOptionProjection {
            option_id: option.option_id.to_string(),
            name: option.name.clone(),
            kind: acp_permission_option_kind_label(option.kind),
        })
        .collect::<Vec<_>>();

    AcpPermissionRequestProjection {
        tool_call_id,
        name,
        arguments,
        summary: "ACP agent requested permission.".to_string(),
        risk: "execute".to_string(),
        options,
    }
}

pub fn acp_permission_response_for_decision(
    request: &RequestPermissionRequest,
    approved: bool,
) -> RequestPermissionResponse {
    let option_id = acp_permission_option_id_for_decision(request, approved);
    option_id
        .map(|id| {
            RequestPermissionResponse::new(RequestPermissionOutcome::Selected(
                SelectedPermissionOutcome::new(id),
            ))
        })
        .unwrap_or_else(acp_permission_cancelled_response)
}

pub fn acp_permission_cancelled_response() -> RequestPermissionResponse {
    RequestPermissionResponse::new(RequestPermissionOutcome::Cancelled)
}

pub fn acp_method_not_found(method: &'static str) -> agent_client_protocol::Error {
    agent_client_protocol::Error::method_not_found().data(method)
}

pub async fn resolve_acp_read_text_file_request(
    workspace_root: &Path,
    request: &ReadTextFileRequest,
) -> Result<ReadTextFileResponse, agent_client_protocol::Error> {
    if !request.path.is_absolute() {
        return Err(agent_client_protocol::util::internal_error(
            "ACP fs/read_text_file requires an absolute path",
        ));
    }
    let root = tokio::fs::canonicalize(workspace_root)
        .await
        .map_err(agent_client_protocol::Error::into_internal_error)?;
    let path = tokio::fs::canonicalize(&request.path)
        .await
        .map_err(agent_client_protocol::Error::into_internal_error)?;
    if !path.starts_with(&root) {
        return Err(agent_client_protocol::util::internal_error(
            "ACP fs/read_text_file path is outside the session root",
        ));
    }
    let content = tokio::fs::read_to_string(path)
        .await
        .map_err(agent_client_protocol::Error::into_internal_error)?;
    Ok(ReadTextFileResponse::new(apply_acp_read_text_line_range(
        &content,
        request.line,
        request.limit,
    )))
}

pub async fn resolve_acp_write_text_file_request(
    workspace_root: &Path,
    request: &WriteTextFileRequest,
) -> Result<WriteTextFileResponse, agent_client_protocol::Error> {
    let target_path = resolve_acp_write_target_path(workspace_root, &request.path).await?;
    // The ACP payload can contain sensitive file contents, so only the validated
    // path crosses this boundary and the content is never logged or formatted.
    tokio::fs::write(target_path, request.content.as_bytes())
        .await
        .map_err(agent_client_protocol::Error::into_internal_error)?;
    Ok(WriteTextFileResponse::new())
}

pub fn acp_client_event_to_ai_stream_events(event: AcpClientEvent) -> Vec<AiStreamEvent> {
    match event {
        AcpClientEvent::SessionUpdate(notification) => {
            acp_session_notification_to_ai_stream_events(&notification)
        }
        AcpClientEvent::RequestPermission { response_tx, .. } => {
            reject_acp_client_request(response_tx, "session/request_permission");
            Vec::new()
        }
        AcpClientEvent::ReadTextFile { response_tx, .. } => {
            reject_acp_client_request(response_tx, "fs/read_text_file");
            Vec::new()
        }
        AcpClientEvent::WriteTextFile { response_tx, .. } => {
            reject_acp_client_request(response_tx, "fs/write_text_file");
            Vec::new()
        }
        AcpClientEvent::CreateTerminal { response_tx, .. } => {
            reject_acp_client_request(response_tx, "terminal/create");
            Vec::new()
        }
        AcpClientEvent::TerminalOutput { response_tx, .. } => {
            reject_acp_client_request(response_tx, "terminal/output");
            Vec::new()
        }
        AcpClientEvent::ReleaseTerminal { response_tx, .. } => {
            reject_acp_client_request(response_tx, "terminal/release");
            Vec::new()
        }
        AcpClientEvent::WaitForTerminalExit { response_tx, .. } => {
            reject_acp_client_request(response_tx, "terminal/wait_for_exit");
            Vec::new()
        }
        AcpClientEvent::KillTerminal { response_tx, .. } => {
            reject_acp_client_request(response_tx, "terminal/kill");
            Vec::new()
        }
    }
}

#[derive(Clone, Default)]
pub struct AcpTerminalRegistry {
    terminals: Arc<Mutex<HashMap<String, AcpManagedTerminal>>>,
}

#[derive(Clone)]
struct AcpManagedTerminal {
    output: Arc<Mutex<AcpTerminalOutput>>,
    status_rx: watch::Receiver<Option<TerminalExitStatus>>,
    control_tx: mpsc::UnboundedSender<AcpTerminalControl>,
}

#[derive(Default)]
struct AcpTerminalOutput {
    content: String,
    byte_limit: Option<usize>,
    truncated: bool,
}

enum AcpTerminalControl {
    Kill,
    Release,
}

impl AcpTerminalRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn create_terminal(
        &self,
        workspace_root: &Path,
        request: &CreateTerminalRequest,
    ) -> Result<CreateTerminalResponse, agent_client_protocol::Error> {
        if request.command.trim().is_empty() {
            return Err(agent_client_protocol::util::internal_error(
                "ACP terminal/create requires a command",
            ));
        }
        let cwd = resolve_acp_terminal_cwd(workspace_root, request.cwd.as_deref()).await?;
        let terminal_id = format!(
            "acp-terminal-{}",
            ACP_TERMINAL_COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let mut command = tokio::process::Command::new(request.command.trim());
        configure_acp_tokio_process(&mut command);
        command.args(&request.args);
        command.current_dir(cwd);
        for variable in &request.env {
            command.env(&variable.name, &variable.value);
        }
        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(agent_client_protocol::Error::into_internal_error)?;
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let output = Arc::new(Mutex::new(AcpTerminalOutput {
            content: String::new(),
            byte_limit: request
                .output_byte_limit
                .and_then(|limit| limit.try_into().ok()),
            truncated: false,
        }));
        if let Some(stdout) = stdout {
            tokio::spawn(read_acp_terminal_output(stdout, output.clone()));
        }
        if let Some(stderr) = stderr {
            tokio::spawn(read_acp_terminal_output(stderr, output.clone()));
        }

        let (control_tx, mut control_rx) = mpsc::unbounded_channel();
        let (status_tx, status_rx) = watch::channel(None);
        tokio::spawn(async move {
            // The runner owns the child so wait/kill/release cannot race over a
            // borrowed process handle.
            let exit_status = tokio::select! {
                result = child.wait() => result.map(acp_terminal_exit_status),
                command = control_rx.recv() => {
                    if matches!(command, Some(AcpTerminalControl::Kill | AcpTerminalControl::Release)) {
                        let _ = child.kill().await;
                    }
                    child.wait().await.map(acp_terminal_exit_status)
                }
            };
            let _ = status_tx.send(Some(
                exit_status.unwrap_or_else(|_| TerminalExitStatus::new()),
            ));
        });

        self.terminals.lock().await.insert(
            terminal_id.clone(),
            AcpManagedTerminal {
                output,
                status_rx,
                control_tx,
            },
        );
        Ok(CreateTerminalResponse::new(terminal_id))
    }

    pub async fn terminal_output(
        &self,
        request: &TerminalOutputRequest,
    ) -> Result<TerminalOutputResponse, agent_client_protocol::Error> {
        let terminal = self.terminal(&request.terminal_id.to_string()).await?;
        let output = terminal.output.lock().await;
        Ok(
            TerminalOutputResponse::new(output.content.clone(), output.truncated)
                .exit_status(terminal.status_rx.borrow().clone()),
        )
    }

    pub async fn release_terminal(
        &self,
        request: &ReleaseTerminalRequest,
    ) -> Result<ReleaseTerminalResponse, agent_client_protocol::Error> {
        let terminal = self
            .terminals
            .lock()
            .await
            .remove(&request.terminal_id.to_string())
            .ok_or_else(acp_terminal_not_found)?;
        let _ = terminal.control_tx.send(AcpTerminalControl::Release);
        Ok(ReleaseTerminalResponse::new())
    }

    pub async fn wait_for_terminal_exit(
        &self,
        request: &WaitForTerminalExitRequest,
    ) -> Result<WaitForTerminalExitResponse, agent_client_protocol::Error> {
        let terminal = self.terminal(&request.terminal_id.to_string()).await?;
        let mut status_rx = terminal.status_rx.clone();
        loop {
            if let Some(status) = status_rx.borrow().clone() {
                return Ok(WaitForTerminalExitResponse::new(status));
            }
            status_rx
                .changed()
                .await
                .map_err(agent_client_protocol::Error::into_internal_error)?;
        }
    }

    pub async fn kill_terminal(
        &self,
        request: &KillTerminalRequest,
    ) -> Result<KillTerminalResponse, agent_client_protocol::Error> {
        let terminal = self.terminal(&request.terminal_id.to_string()).await?;
        let _ = terminal.control_tx.send(AcpTerminalControl::Kill);
        Ok(KillTerminalResponse::new())
    }

    async fn terminal(
        &self,
        terminal_id: &str,
    ) -> Result<AcpManagedTerminal, agent_client_protocol::Error> {
        self.terminals
            .lock()
            .await
            .get(terminal_id)
            .cloned()
            .ok_or_else(acp_terminal_not_found)
    }
}

fn acp_permission_option_id_for_decision(
    request: &RequestPermissionRequest,
    approved: bool,
) -> Option<String> {
    let preferred = if approved {
        [
            PermissionOptionKind::AllowOnce,
            PermissionOptionKind::AllowAlways,
        ]
    } else {
        [
            PermissionOptionKind::RejectOnce,
            PermissionOptionKind::RejectAlways,
        ]
    };
    preferred.iter().find_map(|kind| {
        request
            .options
            .iter()
            .find(|option| option.kind == *kind)
            .map(|option| option.option_id.to_string())
    })
}

fn acp_permission_option_kind_label(kind: PermissionOptionKind) -> &'static str {
    match kind {
        PermissionOptionKind::AllowOnce => "allow_once",
        PermissionOptionKind::AllowAlways => "allow_always",
        PermissionOptionKind::RejectOnce => "reject_once",
        PermissionOptionKind::RejectAlways => "reject_always",
        _ => "unknown",
    }
}

fn apply_acp_read_text_line_range(content: &str, line: Option<u32>, limit: Option<u32>) -> String {
    if line.is_none() && limit.is_none() {
        return content.to_string();
    }
    let start = line.unwrap_or(1).max(1).saturating_sub(1) as usize;
    let mut lines = content.lines().skip(start);
    match limit {
        Some(limit) => lines
            .by_ref()
            .take(limit as usize)
            .collect::<Vec<_>>()
            .join("\n"),
        None => lines.collect::<Vec<_>>().join("\n"),
    }
}

async fn resolve_acp_write_target_path(
    workspace_root: &Path,
    requested_path: &Path,
) -> Result<PathBuf, agent_client_protocol::Error> {
    if !requested_path.is_absolute() {
        return Err(agent_client_protocol::util::internal_error(
            "ACP fs/write_text_file requires an absolute path",
        ));
    }
    let root = tokio::fs::canonicalize(workspace_root)
        .await
        .map_err(agent_client_protocol::Error::into_internal_error)?;
    if tokio::fs::try_exists(requested_path)
        .await
        .map_err(agent_client_protocol::Error::into_internal_error)?
    {
        let existing_path = tokio::fs::canonicalize(requested_path)
            .await
            .map_err(agent_client_protocol::Error::into_internal_error)?;
        if !existing_path.starts_with(&root) {
            return Err(agent_client_protocol::util::internal_error(
                "ACP fs/write_text_file path is outside the session root",
            ));
        }
        return Ok(requested_path.to_path_buf());
    }
    let parent = requested_path.parent().ok_or_else(|| {
        agent_client_protocol::util::internal_error("ACP fs/write_text_file path has no parent")
    })?;
    let parent = tokio::fs::canonicalize(parent)
        .await
        .map_err(agent_client_protocol::Error::into_internal_error)?;
    if !parent.starts_with(&root) {
        return Err(agent_client_protocol::util::internal_error(
            "ACP fs/write_text_file path is outside the session root",
        ));
    }
    Ok(requested_path.to_path_buf())
}

async fn resolve_acp_terminal_cwd(
    workspace_root: &Path,
    requested_cwd: Option<&Path>,
) -> Result<PathBuf, agent_client_protocol::Error> {
    let root = tokio::fs::canonicalize(workspace_root)
        .await
        .map_err(agent_client_protocol::Error::into_internal_error)?;
    let cwd = match requested_cwd {
        Some(cwd) if !cwd.is_absolute() => {
            return Err(agent_client_protocol::util::internal_error(
                "ACP terminal/create cwd must be absolute",
            ));
        }
        Some(cwd) => tokio::fs::canonicalize(cwd)
            .await
            .map_err(agent_client_protocol::Error::into_internal_error)?,
        None => root.clone(),
    };
    if !cwd.starts_with(&root) {
        return Err(agent_client_protocol::util::internal_error(
            "ACP terminal/create cwd is outside the session root",
        ));
    }
    Ok(cwd)
}

async fn read_acp_terminal_output(
    mut reader: impl tokio::io::AsyncRead + Unpin,
    output: Arc<Mutex<AcpTerminalOutput>>,
) {
    let mut buffer = [0_u8; 8192];
    loop {
        match reader.read(&mut buffer).await {
            Ok(0) | Err(_) => return,
            Ok(read) => {
                let chunk = String::from_utf8_lossy(&buffer[..read]);
                let mut output = output.lock().await;
                output.content.push_str(&chunk);
                truncate_acp_terminal_output(&mut output);
            }
        }
    }
}

fn truncate_acp_terminal_output(output: &mut AcpTerminalOutput) {
    let Some(byte_limit) = output.byte_limit else {
        return;
    };
    if output.content.len() <= byte_limit {
        return;
    }
    let mut start = output.content.len().saturating_sub(byte_limit);
    while start < output.content.len() && !output.content.is_char_boundary(start) {
        start += 1;
    }
    output.content = output.content[start..].to_string();
    output.truncated = true;
}

fn acp_terminal_exit_status(status: std::process::ExitStatus) -> TerminalExitStatus {
    let mut exit_status = TerminalExitStatus::new();
    if let Some(code) = status.code().and_then(|code| u32::try_from(code).ok()) {
        exit_status = exit_status.exit_code(Some(code));
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt as _;
        if let Some(signal) = status.signal() {
            exit_status = exit_status.signal(Some(signal.to_string()));
        }
    }
    exit_status
}

fn acp_terminal_not_found() -> agent_client_protocol::Error {
    agent_client_protocol::util::internal_error("ACP terminal id was not found")
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct AcpRuntimeHandleKey {
    pub conversation_id: String,
    pub generation_id: String,
    pub session_id: String,
}

#[derive(Clone, Debug, Default)]
pub struct AcpRuntimeRegistry {
    handles: Arc<parking_lot::Mutex<HashMap<AcpRuntimeHandleKey, AcpRuntimeHandle>>>,
}

#[derive(Clone, Debug)]
struct AcpRuntimeHandle {
    command_tx: mpsc::UnboundedSender<AcpRuntimeCommand>,
}

#[derive(Clone, Debug)]
enum AcpRuntimeCommand {
    CancelSession,
}

#[derive(Debug)]
pub struct AcpRegisteredRuntimeHandle {
    key: AcpRuntimeHandleKey,
    registry: AcpRuntimeRegistry,
}

struct AcpChildGuard(async_process::Child);

impl AcpChildGuard {
    async fn status(&mut self) -> std::io::Result<std::process::ExitStatus> {
        self.0.status().await
    }
}

impl Drop for AcpChildGuard {
    fn drop(&mut self) {
        // Ensure Stop/drop paths do not leave local ACP agent processes alive.
        drop(self.0.kill());
    }
}

impl AcpStdioLauncher {
    pub fn config(&self) -> &AcpLaunchConfig {
        &self.config
    }

    fn spawn_process(
        &self,
    ) -> Result<
        (
            async_process::ChildStdin,
            async_process::ChildStdout,
            Option<async_process::ChildStderr>,
            async_process::Child,
        ),
        agent_client_protocol::Error,
    > {
        let command_path = resolve_acp_command(self.config.command.trim());
        let mut command = async_process::Command::new(command_path);
        configure_acp_async_process(&mut command);
        command.args(&self.config.args);
        command.envs(&self.config.env);
        if let Some(cwd) = &self.config.cwd {
            command.current_dir(cwd);
        }
        command
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = command
            .spawn()
            .map_err(agent_client_protocol::Error::into_internal_error)?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| agent_client_protocol::util::internal_error("Failed to open stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| agent_client_protocol::util::internal_error("Failed to open stdout"))?;
        let stderr = child.stderr.take();
        Ok((stdin, stdout, stderr, child))
    }
}

fn configure_acp_tokio_process(command: &mut tokio::process::Command) {
    #[cfg(windows)]
    {
        // ACP managed terminals are protocol-managed background processes, not
        // visible OxideTerm panes. Keep their captured stdio off the desktop.
        command.creation_flags(ACP_BACKGROUND_PROCESS_CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        let _ = command;
    }
}

fn configure_acp_async_process(command: &mut async_process::Command) {
    #[cfg(windows)]
    {
        // ACP stdio agents communicate over captured pipes and should not flash
        // a console when spawned from the Windows GUI app.
        command.creation_flags(ACP_BACKGROUND_PROCESS_CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        let _ = command;
    }
}

impl<R: Role> ConnectTo<R> for AcpStdioLauncher {
    async fn connect_to(
        self,
        client: impl ConnectTo<R::Counterpart>,
    ) -> Result<(), agent_client_protocol::Error> {
        let (stdin, stdout, stderr, child) = self.spawn_process()?;
        let mut child = AcpChildGuard(child);
        let stderr_future = async move {
            if let Some(stderr) = stderr {
                let mut lines = futures::io::BufReader::new(stderr).lines();
                while lines.next().await.is_some() {
                    // Drain stderr so the child cannot block on a full pipe.
                }
            }
        };
        let incoming = Box::pin(futures::io::BufReader::new(stdout).lines());
        let outgoing = Box::pin(futures::sink::unfold(
            stdin,
            async move |mut writer, line: String| {
                writer.write_all(line.as_bytes()).await?;
                writer.write_all(b"\n").await?;
                Ok::<_, std::io::Error>(writer)
            },
        ));
        let protocol = agent_client_protocol::ConnectTo::<R>::connect_to(
            Lines::new(outgoing, incoming),
            client,
        );
        let child_monitor = async move {
            let status = child
                .status()
                .await
                .map_err(agent_client_protocol::Error::into_internal_error)?;
            Err(agent_client_protocol::util::internal_error(format!(
                "ACP agent process exited with status {status}"
            )))
        };
        let protocol = protocol.fuse();
        let child_monitor = child_monitor.fuse();
        pin_mut!(protocol, child_monitor);
        let main = async move {
            futures::select! {
                result = protocol => result,
                result = child_monitor => result,
            }
        };
        let main = main.fuse();
        let stderr_future = stderr_future.fuse();
        pin_mut!(main, stderr_future);
        futures::select! {
            result = main => result,
            () = stderr_future => main.await,
        }
    }
}

impl fmt::Debug for AcpLaunchConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AcpLaunchConfig")
            .field("id", &self.id)
            .field("display_name", &self.display_name)
            .field("command", &self.command)
            // Args and env values can include tokens passed to local ACP tools.
            .field("args", &format_args!("<redacted:{}>", self.args.len()))
            .field("env", &format_args!("<redacted:{}>", self.env.len()))
            .field("cwd", &self.cwd)
            .finish()
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum AcpLaunchConfigError {
    #[error("ACP agent command is required")]
    EmptyCommand,
    #[error("ACP agent command contains a NUL byte")]
    CommandContainsNul,
    #[error("ACP agent environment variable name is invalid")]
    InvalidEnvName,
    #[error("ACP agent cwd requires the custom stdio launcher")]
    CwdRequiresCustomLauncher,
}

pub fn build_sdk_acp_agent(config: &AcpLaunchConfig) -> Result<AcpAgent, AcpLaunchConfigError> {
    validate_launch_config(config)?;
    if config.cwd.is_some() {
        // The SDK AcpAgent wrapper does not expose current_dir. Full runtime
        // support must use a custom SDK ConnectTo launcher for cwd-aware agents.
        return Err(AcpLaunchConfigError::CwdRequiresCustomLauncher);
    }

    let command = config.command.trim();
    let env = acp_env_variables(config)?;
    let name = acp_agent_name(config);
    let server = McpServer::Stdio(
        McpServerStdio::new(name, command)
            .args(config.args.clone())
            .env(env),
    );
    Ok(AcpAgent::new(server))
}

pub fn build_acp_stdio_launcher(
    config: AcpLaunchConfig,
) -> Result<AcpStdioLauncher, AcpLaunchConfigError> {
    validate_launch_config(&config)?;
    acp_env_variables(&config)?;
    Ok(AcpStdioLauncher { config })
}

pub fn acp_launch_command_available(
    config: &AcpLaunchConfig,
) -> Result<bool, AcpLaunchConfigError> {
    validate_launch_config(config)?;
    Ok(resolve_acp_command(config.command.trim()).exists())
}

pub fn build_acp_initialize_request(
    client_version: &str,
    policy: &AcpHostCapabilityPolicy,
) -> InitializeRequest {
    InitializeRequest::new(ProtocolVersion::V1)
        .client_capabilities(
            ClientCapabilities::new()
                .fs(FileSystemCapabilities::new()
                    .read_text_file(policy.fs_read_text_file)
                    .write_text_file(policy.fs_write_text_file))
                .terminal(policy.terminal),
        )
        .client_info(Implementation::new("OxideTerm", client_version))
}

fn ensure_acp_v1_initialize_response(
    response: InitializeResponse,
) -> Result<InitializeResponse, agent_client_protocol::Error> {
    if response.protocol_version == ProtocolVersion::V1 {
        Ok(response)
    } else {
        // OxideTerm's ACP client surface is defined for v1; continuing after a
        // pre-release or draft response would make capability checks ambiguous.
        Err(agent_client_protocol::util::internal_error(
            "ACP agent returned unsupported protocol version",
        ))
    }
}

pub async fn initialize_acp_agent(
    transport: impl ConnectTo<Client> + 'static,
    client_version: String,
    policy: AcpHostCapabilityPolicy,
) -> Result<InitializeResponse, agent_client_protocol::Error> {
    Client
        .builder()
        .name("OxideTerm")
        .connect_with(transport, async move |connection: ConnectionTo<Agent>| {
            let response = connection
                .send_request(build_acp_initialize_request(&client_version, &policy))
                .block_task()
                .await?;
            ensure_acp_v1_initialize_response(response)
        })
        .await
}

pub async fn with_acp_agent_runtime<R>(
    transport: impl ConnectTo<Client> + 'static,
    client_version: String,
    policy: AcpHostCapabilityPolicy,
    op: impl AsyncFnOnce(AcpAgentRuntime) -> Result<R, agent_client_protocol::Error>,
) -> Result<R, agent_client_protocol::Error> {
    Client
        .builder()
        .name("OxideTerm")
        .connect_with(transport, async move |connection: ConnectionTo<Agent>| {
            let initialize_response = connection
                .send_request(build_acp_initialize_request(&client_version, &policy))
                .block_task()
                .await?;
            let initialize_response = ensure_acp_v1_initialize_response(initialize_response)?;
            op(AcpAgentRuntime {
                connection,
                initialize_response,
            })
            .await
        })
        .await
}

/// Starts a disposable ACP session and returns only its serializable configuration snapshot.
/// The session identifier is intentionally discarded because the backing stdio process exits
/// when this discovery operation completes.
pub async fn discover_acp_session_config_options(
    transport: impl ConnectTo<Client> + 'static,
    client_version: String,
    policy: AcpHostCapabilityPolicy,
    cwd: PathBuf,
) -> Result<Vec<AcpSessionConfigOption>, agent_client_protocol::Error> {
    with_acp_agent_runtime(transport, client_version, policy, async move |runtime| {
        let session = runtime
            .start_or_resume_session(None, cwd, Vec::new())
            .await?;
        Ok(acp_session_config_options(session.config_options()))
    })
    .await
}

pub async fn with_acp_agent_runtime_events<R>(
    transport: impl ConnectTo<Client> + 'static,
    client_version: String,
    policy: AcpHostCapabilityPolicy,
    event_tx: AcpClientEventSender,
    op: impl AsyncFnOnce(AcpAgentRuntime) -> Result<R, agent_client_protocol::Error>,
) -> Result<R, agent_client_protocol::Error> {
    let session_update_tx = event_tx.clone();
    let request_permission_tx = event_tx.clone();
    let read_text_file_tx = event_tx.clone();
    let write_text_file_tx = event_tx.clone();
    let create_terminal_tx = event_tx.clone();
    let terminal_output_tx = event_tx.clone();
    let release_terminal_tx = event_tx.clone();
    let wait_for_terminal_exit_tx = event_tx.clone();
    let kill_terminal_tx = event_tx;

    Client
        .builder()
        .name("OxideTerm")
        .on_receive_notification(
            async move |notification: SessionNotification, _connection| {
                send_client_event(
                    &session_update_tx,
                    AcpClientEvent::SessionUpdate(notification),
                )
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_request(
            async move |request: RequestPermissionRequest, responder, _connection| {
                let response = forward_client_request(&request_permission_tx, |response_tx| {
                    AcpClientEvent::RequestPermission {
                        request,
                        response_tx,
                    }
                })
                .await;
                responder.respond_with_result(response)
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: ReadTextFileRequest, responder, _connection| {
                let response = forward_client_request(&read_text_file_tx, |response_tx| {
                    AcpClientEvent::ReadTextFile {
                        request,
                        response_tx,
                    }
                })
                .await;
                responder.respond_with_result(response)
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: WriteTextFileRequest, responder, _connection| {
                let response = forward_client_request(&write_text_file_tx, |response_tx| {
                    AcpClientEvent::WriteTextFile {
                        request,
                        response_tx,
                    }
                })
                .await;
                responder.respond_with_result(response)
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: CreateTerminalRequest, responder, _connection| {
                let response = forward_client_request(&create_terminal_tx, |response_tx| {
                    AcpClientEvent::CreateTerminal {
                        request,
                        response_tx,
                    }
                })
                .await;
                responder.respond_with_result(response)
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: TerminalOutputRequest, responder, _connection| {
                let response = forward_client_request(&terminal_output_tx, |response_tx| {
                    AcpClientEvent::TerminalOutput {
                        request,
                        response_tx,
                    }
                })
                .await;
                responder.respond_with_result(response)
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: ReleaseTerminalRequest, responder, _connection| {
                let response = forward_client_request(&release_terminal_tx, |response_tx| {
                    AcpClientEvent::ReleaseTerminal {
                        request,
                        response_tx,
                    }
                })
                .await;
                responder.respond_with_result(response)
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: WaitForTerminalExitRequest, responder, _connection| {
                let response = forward_client_request(&wait_for_terminal_exit_tx, |response_tx| {
                    AcpClientEvent::WaitForTerminalExit {
                        request,
                        response_tx,
                    }
                })
                .await;
                responder.respond_with_result(response)
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: KillTerminalRequest, responder, _connection| {
                let response = forward_client_request(&kill_terminal_tx, |response_tx| {
                    AcpClientEvent::KillTerminal {
                        request,
                        response_tx,
                    }
                })
                .await;
                responder.respond_with_result(response)
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_with(transport, async move |connection: ConnectionTo<Agent>| {
            let initialize_response = connection
                .send_request(build_acp_initialize_request(&client_version, &policy))
                .block_task()
                .await?;
            let initialize_response = ensure_acp_v1_initialize_response(initialize_response)?;
            op(AcpAgentRuntime {
                connection,
                initialize_response,
            })
            .await
        })
        .await
}

#[allow(clippy::too_many_arguments)]
pub async fn run_acp_prompt_session_events(
    transport: impl ConnectTo<Client> + 'static,
    client_version: String,
    policy: AcpHostCapabilityPolicy,
    session_cwd: PathBuf,
    existing_session_id: Option<String>,
    config_selection: Option<AcpSessionConfigSelection>,
    mcp_servers: Vec<McpServer>,
    prompt: String,
    event_tx: AcpClientEventSender,
    registry: AcpRuntimeRegistry,
    conversation_id: String,
    generation_id: String,
) -> Result<AcpPromptSessionOutcome, agent_client_protocol::Error> {
    let latest_config_options = Arc::new(Mutex::new(None));
    let relay_config_options = Arc::clone(&latest_config_options);
    let (runtime_event_tx, mut runtime_event_rx) = mpsc::unbounded_channel();
    let event_relay = tokio::spawn(async move {
        while let Some(event) = runtime_event_rx.recv().await {
            if let AcpClientEvent::SessionUpdate(notification) = &event
                && let SessionUpdate::ConfigOptionUpdate(update) = &notification.update
            {
                // The notification is a complete snapshot and may race the prompt response.
                *relay_config_options.lock().await = Some((
                    notification.session_id.to_string(),
                    acp_session_config_options(&update.config_options),
                ));
            }
            if event_tx.send(event).is_err() {
                break;
            }
        }
    });
    let session_event_tx = runtime_event_tx.clone();
    let result = with_acp_agent_runtime_events(
        transport,
        client_version,
        policy,
        runtime_event_tx,
        async move |runtime| {
            let mut session = runtime
                .start_or_resume_session(existing_session_id, session_cwd, mcp_servers)
                .await?;
            let session_id = session.session_id().to_string();
            let session_metadata = session.meta().clone().map(serde_json::Value::Object);
            if let Some(selection) =
                config_selection.filter(|selection| session.supports_config_selection(selection))
            {
                let response = runtime
                    .set_session_config_option(
                        session_id.clone(),
                        selection.config_id,
                        selection.value_id,
                    )
                    .await?;
                // The response is an authoritative full snapshot, not a patch.
                session.replace_config_options(response.config_options);
            }
            let _registered_handle = runtime.register_session_handle(
                registry,
                AcpRuntimeHandleKey {
                    conversation_id,
                    generation_id,
                    session_id: session_id.clone(),
                },
            )?;
            session.send_prompt(prompt)?;
            forward_acp_session_updates_to_client_events(&mut session, &session_event_tx).await?;
            let session_config_options = acp_session_config_options(session.config_options());
            Ok(AcpPromptSessionOutcome {
                session_id,
                session_metadata,
                session_config_options,
            })
        },
    )
    .await;
    let _ = event_relay.await;
    match result {
        Ok(mut outcome) => {
            if let Some((session_id, options)) = latest_config_options.lock().await.take()
                && session_id == outcome.session_id
            {
                outcome.session_config_options = options;
            }
            Ok(outcome)
        }
        Err(error) => Err(error),
    }
}

impl AcpAgentRuntime {
    pub fn initialize_response(&self) -> &InitializeResponse {
        &self.initialize_response
    }

    pub fn agent_capabilities(&self) -> &AgentCapabilities {
        &self.initialize_response.agent_capabilities
    }

    pub fn auth_methods(&self) -> &[AuthMethod] {
        &self.initialize_response.auth_methods
    }

    pub async fn authenticate(
        &self,
        method_id: impl Into<AuthMethodId>,
    ) -> Result<AuthenticateResponse, agent_client_protocol::Error> {
        let method_id = method_id.into();
        let method_supported = self
            .initialize_response
            .auth_methods
            .iter()
            .any(|method| method.id() == &method_id);
        ensure_negotiated(method_supported, "authenticate")?;
        self.connection
            .send_request(AuthenticateRequest::new(method_id))
            .block_task()
            .await
    }

    pub async fn logout(&self) -> Result<LogoutResponse, agent_client_protocol::Error> {
        ensure_negotiated(
            self.initialize_response
                .agent_capabilities
                .auth
                .logout
                .is_some(),
            "logout",
        )?;
        self.connection
            .send_request(LogoutRequest::new())
            .block_task()
            .await
    }

    pub async fn start_session(
        &self,
        request: NewSessionRequest,
    ) -> Result<AcpActiveSession, agent_client_protocol::Error> {
        self.ensure_additional_directories_allowed(&request.additional_directories)?;
        // Preserve configOptions before the SDK's ActiveSession drops the field.
        let response = self.connection.send_request(request).block_task().await?;
        self.attach_session(response)
    }

    pub async fn start_or_resume_session(
        &self,
        existing_session_id: Option<String>,
        cwd: PathBuf,
        mcp_servers: Vec<McpServer>,
    ) -> Result<AcpActiveSession, agent_client_protocol::Error> {
        if let Some(session_id) = existing_session_id.filter(|id| !id.trim().is_empty()) {
            if self
                .initialize_response
                .agent_capabilities
                .session_capabilities
                .resume
                .is_some()
            {
                if let Ok(response) = self
                    .resume_session(
                        ResumeSessionRequest::new(session_id.clone(), cwd.clone())
                            .mcp_servers(mcp_servers.clone()),
                    )
                    .await
                {
                    return self.attach_existing_session(
                        session_id,
                        response.modes,
                        response.config_options,
                        response.meta,
                    );
                }
            }
            if self.initialize_response.agent_capabilities.load_session {
                if let Ok(response) = self
                    .load_session(
                        LoadSessionRequest::new(session_id.clone(), cwd.clone())
                            .mcp_servers(mcp_servers.clone()),
                    )
                    .await
                {
                    return self.attach_existing_session(
                        session_id,
                        response.modes,
                        response.config_options,
                        response.meta,
                    );
                }
            }
        }

        self.start_session(NewSessionRequest::new(cwd).mcp_servers(mcp_servers))
            .await
    }

    fn attach_existing_session(
        &self,
        session_id: String,
        modes: Option<agent_client_protocol::schema::SessionModeState>,
        config_options: Option<Vec<SessionConfigOption>>,
        meta: Option<agent_client_protocol::schema::Meta>,
    ) -> Result<AcpActiveSession, agent_client_protocol::Error> {
        // The SDK's active update handler is attached through
        // NewSessionResponse; load/resume responses carry the same state
        // except for the id, which comes from persisted conversation metadata.
        let response = NewSessionResponse::new(session_id)
            .modes(modes)
            .config_options(config_options)
            .meta(meta);
        self.attach_session(response)
    }

    fn attach_session(
        &self,
        response: NewSessionResponse,
    ) -> Result<AcpActiveSession, agent_client_protocol::Error> {
        let config_options = response.config_options.clone().unwrap_or_default();
        let inner = self.connection.attach_session(response, Vec::new())?;
        Ok(AcpActiveSession {
            inner,
            config_options,
        })
    }

    pub async fn load_session(
        &self,
        request: LoadSessionRequest,
    ) -> Result<LoadSessionResponse, agent_client_protocol::Error> {
        ensure_negotiated(
            self.initialize_response.agent_capabilities.load_session,
            "session/load",
        )?;
        self.ensure_additional_directories_allowed(&request.additional_directories)?;
        self.connection.send_request(request).block_task().await
    }

    pub async fn resume_session(
        &self,
        request: ResumeSessionRequest,
    ) -> Result<ResumeSessionResponse, agent_client_protocol::Error> {
        ensure_negotiated(
            self.initialize_response
                .agent_capabilities
                .session_capabilities
                .resume
                .is_some(),
            "session/resume",
        )?;
        self.ensure_additional_directories_allowed(&request.additional_directories)?;
        self.connection.send_request(request).block_task().await
    }

    pub async fn list_sessions(
        &self,
        request: ListSessionsRequest,
    ) -> Result<ListSessionsResponse, agent_client_protocol::Error> {
        ensure_negotiated(
            self.initialize_response
                .agent_capabilities
                .session_capabilities
                .list
                .is_some(),
            "session/list",
        )?;
        self.connection.send_request(request).block_task().await
    }

    pub async fn delete_session(
        &self,
        request: DeleteSessionRequest,
    ) -> Result<DeleteSessionResponse, agent_client_protocol::Error> {
        ensure_negotiated(
            self.initialize_response
                .agent_capabilities
                .session_capabilities
                .delete
                .is_some(),
            "session/delete",
        )?;
        self.connection.send_request(request).block_task().await
    }

    pub async fn close_session(
        &self,
        session_id: impl Into<SessionId>,
    ) -> Result<CloseSessionResponse, agent_client_protocol::Error> {
        ensure_negotiated(
            self.initialize_response
                .agent_capabilities
                .session_capabilities
                .close
                .is_some(),
            "session/close",
        )?;
        self.connection
            .send_request(CloseSessionRequest::new(session_id))
            .block_task()
            .await
    }

    pub fn cancel_session(
        &self,
        session_id: impl Into<SessionId>,
    ) -> Result<(), agent_client_protocol::Error> {
        self.connection
            .send_notification(CancelNotification::new(session_id))
    }

    pub fn register_session_handle(
        &self,
        registry: AcpRuntimeRegistry,
        key: AcpRuntimeHandleKey,
    ) -> Result<AcpRegisteredRuntimeHandle, agent_client_protocol::Error> {
        let (command_tx, mut command_rx) = mpsc::unbounded_channel();
        let connection = self.connection.clone();
        let session_id = key.session_id.clone();
        connection.clone().spawn(async move {
            while let Some(command) = command_rx.recv().await {
                match command {
                    AcpRuntimeCommand::CancelSession => {
                        // Stop uses the SDK connection that owns the session so
                        // cancellation reaches the right ACP agent process.
                        connection
                            .send_notification(CancelNotification::new(session_id.clone()))?;
                    }
                }
            }
            Ok(())
        })?;
        registry.insert_handle(key.clone(), AcpRuntimeHandle { command_tx });
        Ok(AcpRegisteredRuntimeHandle { key, registry })
    }

    pub async fn set_session_mode(
        &self,
        session_id: impl Into<SessionId>,
        mode_id: impl Into<SessionModeId>,
    ) -> Result<SetSessionModeResponse, agent_client_protocol::Error> {
        // Modes are negotiated per session in NewSessionResponse/ResumeSessionResponse.
        self.connection
            .send_request(SetSessionModeRequest::new(session_id, mode_id))
            .block_task()
            .await
    }

    pub async fn set_session_config_option(
        &self,
        session_id: impl Into<SessionId>,
        config_id: impl Into<SessionConfigId>,
        value: impl Into<SessionConfigValueId>,
    ) -> Result<SetSessionConfigOptionResponse, agent_client_protocol::Error> {
        // Config options are negotiated per session in NewSessionResponse/ResumeSessionResponse.
        self.connection
            .send_request(SetSessionConfigOptionRequest::new(
                session_id, config_id, value,
            ))
            .block_task()
            .await
    }

    fn ensure_additional_directories_allowed(
        &self,
        additional_directories: &[PathBuf],
    ) -> Result<(), agent_client_protocol::Error> {
        ensure_negotiated(
            additional_directories.is_empty()
                || self
                    .initialize_response
                    .agent_capabilities
                    .session_capabilities
                    .additional_directories
                    .is_some(),
            "session/additionalDirectories",
        )
    }
}

impl AcpActiveSession {
    pub fn session_id(&self) -> &SessionId {
        self.inner.session_id()
    }

    pub fn meta(&self) -> &Option<agent_client_protocol::schema::Meta> {
        self.inner.meta()
    }

    pub fn config_options(&self) -> &[SessionConfigOption] {
        &self.config_options
    }

    pub fn send_prompt(
        &mut self,
        prompt: impl ToString,
    ) -> Result<(), agent_client_protocol::Error> {
        self.inner.send_prompt(prompt)
    }

    pub async fn read_update(&mut self) -> Result<SessionMessage, agent_client_protocol::Error> {
        self.inner.read_update().await
    }

    fn replace_config_options(&mut self, options: Vec<SessionConfigOption>) {
        self.config_options = options;
    }

    fn supports_config_selection(&self, selection: &AcpSessionConfigSelection) -> bool {
        self.config_options.iter().any(|option| {
            option.id.to_string() == selection.config_id
                && match &option.kind {
                    SessionConfigKind::Select(select) => match &select.options {
                        SessionConfigSelectOptions::Ungrouped(options) => options
                            .iter()
                            .any(|choice| choice.value.to_string() == selection.value_id),
                        SessionConfigSelectOptions::Grouped(groups) => groups.iter().any(|group| {
                            group
                                .options
                                .iter()
                                .any(|choice| choice.value.to_string() == selection.value_id)
                        }),
                        _ => false,
                    },
                    _ => false,
                }
        })
    }
}

impl AcpRuntimeHandleKey {
    pub fn new(
        conversation_id: impl Into<String>,
        generation_id: impl Into<String>,
        session_id: impl Into<String>,
    ) -> Self {
        Self {
            conversation_id: conversation_id.into(),
            generation_id: generation_id.into(),
            session_id: session_id.into(),
        }
    }
}

impl AcpRuntimeRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel_session(
        &self,
        key: &AcpRuntimeHandleKey,
    ) -> Result<(), agent_client_protocol::Error> {
        let handle = self.handles.lock().get(key).cloned();
        let Some(handle) = handle else {
            return Ok(());
        };
        handle
            .command_tx
            .send(AcpRuntimeCommand::CancelSession)
            .map_err(|_| agent_client_protocol::util::internal_error("ACP runtime handle closed"))
    }

    pub fn cancel_generation(
        &self,
        conversation_id: &str,
        generation_id: &str,
    ) -> Result<(), agent_client_protocol::Error> {
        let handles = self
            .handles
            .lock()
            .iter()
            .filter(|(key, _)| {
                key.conversation_id == conversation_id && key.generation_id == generation_id
            })
            .map(|(_, handle)| handle.clone())
            .collect::<Vec<_>>();
        for handle in handles {
            handle
                .command_tx
                .send(AcpRuntimeCommand::CancelSession)
                .map_err(|_| {
                    agent_client_protocol::util::internal_error("ACP runtime handle closed")
                })?;
        }
        Ok(())
    }

    pub fn contains(&self, key: &AcpRuntimeHandleKey) -> bool {
        self.handles.lock().contains_key(key)
    }

    fn insert_handle(&self, key: AcpRuntimeHandleKey, handle: AcpRuntimeHandle) {
        self.handles.lock().insert(key, handle);
    }

    fn remove_handle(&self, key: &AcpRuntimeHandleKey) {
        self.handles.lock().remove(key);
    }
}

impl Drop for AcpRegisteredRuntimeHandle {
    fn drop(&mut self) {
        self.registry.remove_handle(&self.key);
    }
}

fn ensure_negotiated(
    supported: bool,
    method: &'static str,
) -> Result<(), agent_client_protocol::Error> {
    if supported {
        Ok(())
    } else {
        Err(agent_client_protocol::Error::method_not_found().data(method))
    }
}

fn send_client_event(
    event_tx: &AcpClientEventSender,
    event: AcpClientEvent,
) -> Result<(), agent_client_protocol::Error> {
    event_tx.send(event).map_err(|_| {
        agent_client_protocol::util::internal_error("ACP client event receiver closed")
    })
}

async fn forward_client_request<T>(
    event_tx: &AcpClientEventSender,
    build_event: impl FnOnce(AcpClientResponseSender<T>) -> AcpClientEvent,
) -> Result<T, agent_client_protocol::Error>
where
    T: Send + 'static,
{
    let (response_tx, response_rx) = oneshot::channel();
    send_client_event(event_tx, build_event(response_tx))?;
    response_rx
        .await
        .map_err(|_| agent_client_protocol::util::internal_error("ACP client response dropped"))?
}

async fn forward_acp_session_updates_to_client_events(
    session: &mut AcpActiveSession,
    event_tx: &AcpClientEventSender,
) -> Result<(), agent_client_protocol::Error> {
    loop {
        match session.read_update().await? {
            SessionMessage::SessionMessage(dispatch) => {
                MatchDispatch::new(dispatch)
                    .if_notification(async |notification: SessionNotification| {
                        if let SessionUpdate::ConfigOptionUpdate(update) = &notification.update {
                            // ACP sends full config snapshots, so replace local state atomically.
                            session.replace_config_options(update.config_options.clone());
                        }
                        send_client_event(event_tx, AcpClientEvent::SessionUpdate(notification))
                    })
                    .await
                    .otherwise_ignore()?;
            }
            SessionMessage::StopReason(_) => break,
            // Future SDK message kinds should not break the current turn.
            _ => {}
        }
    }
    Ok(())
}

fn text_content(content: &agent_client_protocol::schema::ContentBlock) -> Option<&str> {
    match content {
        agent_client_protocol::schema::ContentBlock::Text(text) => Some(text.text.as_str()),
        _ => None,
    }
}

fn acp_tool_call_stream_event(tool_call: &ToolCall, complete: bool) -> AiStreamEvent {
    let id = tool_call.tool_call_id.to_string();
    let name = tool_call.title.clone();
    let arguments = acp_tool_arguments(
        tool_call.raw_input.as_ref(),
        tool_call.raw_output.as_ref(),
        Some(tool_call.status),
        Some(&tool_call.content),
    );
    if complete {
        AiStreamEvent::ToolCallComplete {
            id,
            name,
            arguments,
        }
    } else {
        AiStreamEvent::ToolCall {
            id,
            name,
            arguments,
        }
    }
}

fn acp_tool_call_update_stream_event(update: &ToolCallUpdate) -> AiStreamEvent {
    let id = update.tool_call_id.to_string();
    let name = update
        .fields
        .title
        .clone()
        .unwrap_or_else(|| update.tool_call_id.to_string());
    let arguments = acp_tool_arguments(
        update.fields.raw_input.as_ref(),
        update.fields.raw_output.as_ref(),
        update.fields.status,
        update.fields.content.as_ref(),
    );
    let complete = matches!(
        update.fields.status,
        Some(ToolCallStatus::Completed | ToolCallStatus::Failed)
    );
    if complete {
        AiStreamEvent::ToolCallComplete {
            id,
            name,
            arguments,
        }
    } else {
        AiStreamEvent::ToolCall {
            id,
            name,
            arguments,
        }
    }
}

fn acp_tool_arguments(
    raw_input: Option<&serde_json::Value>,
    raw_output: Option<&serde_json::Value>,
    status: Option<ToolCallStatus>,
    content: Option<&Vec<agent_client_protocol::schema::ToolCallContent>>,
) -> String {
    let mut arguments = serde_json::Map::new();
    if let Some(raw_input) = raw_input {
        arguments.insert("input".to_string(), raw_input.clone());
    }
    if let Some(raw_output) = raw_output {
        arguments.insert("output".to_string(), raw_output.clone());
    }
    if let Some(status) = status {
        arguments.insert(
            "status".to_string(),
            serde_json::to_value(status).unwrap_or_else(|_| serde_json::json!("unknown")),
        );
    }
    if let Some(content) = content.filter(|content| !content.is_empty()) {
        arguments.insert(
            "content".to_string(),
            serde_json::to_value(content).unwrap_or_else(|_| serde_json::Value::Null),
        );
    }
    serde_json::Value::Object(arguments).to_string()
}

fn reject_acp_client_request<T>(response_tx: AcpClientResponseSender<T>, method: &'static str) {
    let _ = response_tx.send(Err(
        agent_client_protocol::Error::method_not_found().data(method)
    ));
}

fn validate_launch_config(config: &AcpLaunchConfig) -> Result<(), AcpLaunchConfigError> {
    let command = config.command.trim();
    if command.is_empty() {
        return Err(AcpLaunchConfigError::EmptyCommand);
    }
    if command.contains('\0') {
        return Err(AcpLaunchConfigError::CommandContainsNul);
    }
    Ok(())
}

fn resolve_acp_command(command: &str) -> PathBuf {
    let command_path = Path::new(command);
    if command_path.components().count() > 1 || command_path.is_absolute() {
        return command_path.to_path_buf();
    }

    // Packaged helper binaries are expected beside the current executable.
    if let Ok(current_exe) = env::current_exe() {
        if let Some(parent) = current_exe.parent() {
            for candidate in acp_command_candidates(parent, command) {
                if candidate.exists() {
                    return candidate;
                }
            }
        }
    }

    if let Some(path_var) = env::var_os("PATH") {
        for search_dir in env::split_paths(&path_var) {
            for candidate in acp_command_candidates(&search_dir, command) {
                if candidate.exists() {
                    return candidate;
                }
            }
        }
    }

    command_path.to_path_buf()
}

fn acp_command_candidates(parent: &Path, command: &str) -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        let has_extension = Path::new(command).extension().is_some();
        if has_extension {
            return vec![parent.join(command)];
        }
        let pathext = env::var_os("PATHEXT")
            .map(|value| {
                value
                    .to_string_lossy()
                    .split(';')
                    .filter(|extension| !extension.trim().is_empty())
                    .map(|extension| extension.trim().to_ascii_lowercase())
                    .collect::<Vec<_>>()
            })
            .filter(|extensions| !extensions.is_empty())
            .unwrap_or_else(|| vec![".exe".to_string(), ".cmd".to_string(), ".bat".to_string()]);
        let mut candidates = Vec::with_capacity(pathext.len() + 1);
        candidates.push(parent.join(command));
        candidates.extend(
            pathext
                .into_iter()
                .map(|extension| parent.join(format!("{command}{extension}"))),
        );
        candidates
    }
    #[cfg(not(windows))]
    {
        vec![parent.join(command)]
    }
}

fn acp_env_variables(config: &AcpLaunchConfig) -> Result<Vec<EnvVariable>, AcpLaunchConfigError> {
    let env = config
        .env
        .iter()
        .map(|(name, value)| {
            if name.trim().is_empty() || name.contains('=') || name.contains('\0') {
                return Err(AcpLaunchConfigError::InvalidEnvName);
            }
            Ok(EnvVariable::new(name.clone(), value.clone()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(env)
}

fn acp_agent_name(config: &AcpLaunchConfig) -> &str {
    if config.display_name.trim().is_empty() {
        config.id.trim()
    } else {
        config.display_name.trim()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::{
        AgentAuthCapabilities, AuthMethodAgent, CloseSessionResponse, ContentBlock, ContentChunk,
        CreateTerminalRequest, LogoutCapabilities, LogoutResponse, NewSessionResponse,
        PermissionOption, PermissionOptionKind, PromptRequest, PromptResponse, ReadTextFileRequest,
        ReleaseTerminalRequest, RequestPermissionRequest, SessionCapabilities,
        SessionCloseCapabilities, SessionConfigSelectGroup, SessionConfigSelectOption,
        SessionResumeCapabilities, StopReason, TerminalOutputRequest, ToolCallUpdate,
        ToolCallUpdateFields, WaitForTerminalExitRequest, WriteTextFileRequest,
    };

    fn launch_config() -> AcpLaunchConfig {
        AcpLaunchConfig {
            id: "codex-local".to_string(),
            display_name: "Codex Local".to_string(),
            command: "codex".to_string(),
            args: vec!["--acp".to_string()],
            env: BTreeMap::from([("API_KEY".to_string(), "env-secret".to_string())]),
            cwd: None,
        }
    }

    #[test]
    fn sdk_agent_uses_structured_stdio_config() {
        let agent = build_sdk_acp_agent(&launch_config()).expect("sdk acp agent");

        let McpServer::Stdio(stdio) = agent.server() else {
            panic!("stdio server");
        };
        assert_eq!(stdio.name, "Codex Local");
        assert_eq!(stdio.command, PathBuf::from("codex"));
        assert_eq!(stdio.args, vec!["--acp"]);
        assert_eq!(stdio.env[0].name, "API_KEY");
        assert_eq!(stdio.env[0].value, "env-secret");
    }

    #[test]
    fn session_config_projection_keeps_model_ids_and_group_order() {
        let option = SessionConfigOption::select(
            "model",
            "Model",
            "model-b",
            vec![
                SessionConfigSelectGroup::new(
                    "fast",
                    "Fast",
                    vec![SessionConfigSelectOption::new("model-a", "Model A")],
                ),
                SessionConfigSelectGroup::new(
                    "capable",
                    "Capable",
                    vec![SessionConfigSelectOption::new("model-b", "Model B")],
                ),
            ],
        )
        .category(SessionConfigOptionCategory::Model);

        let projected = acp_session_config_options(&[option]);

        assert_eq!(projected.len(), 1);
        assert_eq!(projected[0].category.as_deref(), Some("model"));
        assert_eq!(projected[0].current_value_id, "model-b");
        assert_eq!(projected[0].choices[0].value_id, "model-a");
        assert_eq!(projected[0].choices[1].label, "Model B");
        assert_eq!(acp_model_config_option(&projected), projected.first());
    }

    #[test]
    fn model_config_compatibility_accepts_missing_category_only_for_model_identity() {
        let model = AcpSessionConfigOption {
            config_id: "model".to_string(),
            name: "Model".to_string(),
            category: None,
            current_value_id: "model-a".to_string(),
            choices: Vec::new(),
        };
        let mode = AcpSessionConfigOption {
            config_id: "mode".to_string(),
            name: "Mode".to_string(),
            category: None,
            current_value_id: "agent".to_string(),
            choices: Vec::new(),
        };

        assert_eq!(
            acp_model_config_option(&[mode, model.clone()]),
            Some(&model)
        );
    }

    #[test]
    fn launch_model_hint_supports_split_and_inline_flags() {
        assert_eq!(
            acp_launch_model_hint(&["--model".to_string(), "gpt-5.4".to_string()]),
            Some(AcpLaunchModelHint::Fixed("gpt-5.4".to_string()))
        );
        assert_eq!(
            acp_launch_model_hint(&["--model=auto".to_string()]),
            Some(AcpLaunchModelHint::Automatic)
        );
        assert_eq!(acp_launch_model_hint(&["--acp".to_string()]), None);
    }

    #[test]
    fn native_claude_adapter_defers_model_report_until_first_prompt() {
        assert!(acp_model_report_is_deferred_until_first_prompt(&[
            "--acp-adapter".to_string(),
            "claude-code".to_string(),
        ]));
        assert!(!acp_model_report_is_deferred_until_first_prompt(&[
            "--acp".to_string(),
            "--stdio".to_string(),
        ]));
    }

    #[test]
    fn native_codex_adapter_reports_models_during_session_start() {
        assert!(acp_model_report_is_available_during_session_start(&[
            "--acp-adapter".to_string(),
            "codex".to_string(),
        ]));
        assert!(!acp_model_report_is_available_during_session_start(&[
            "--acp-adapter".to_string(),
            "claude-code".to_string(),
        ]));
    }

    #[test]
    fn launch_config_debug_redacts_args_and_env_values() {
        let debug = format!("{:?}", launch_config());

        assert!(debug.contains("<redacted:1>"));
        assert!(!debug.contains("env-secret"));
    }

    #[test]
    fn sdk_agent_rejects_cwd_until_custom_launcher_exists() {
        let mut config = launch_config();
        config.cwd = Some(PathBuf::from("/workspace"));

        assert_eq!(
            build_sdk_acp_agent(&config).unwrap_err(),
            AcpLaunchConfigError::CwdRequiresCustomLauncher
        );
    }

    #[test]
    fn custom_launcher_preserves_cwd_for_runtime_spawn() {
        let mut config = launch_config();
        config.cwd = Some(PathBuf::from("/workspace"));

        let launcher = build_acp_stdio_launcher(config).expect("cwd-aware launcher");

        assert_eq!(
            launcher.config().cwd.as_ref(),
            Some(&PathBuf::from("/workspace"))
        );
    }

    #[test]
    fn initialize_request_starts_with_closed_host_capabilities() {
        let request =
            build_acp_initialize_request("2.0.0-test", &AcpHostCapabilityPolicy::default());

        assert_eq!(request.protocol_version, ProtocolVersion::V1);
        assert!(!request.client_capabilities.fs.read_text_file);
        assert!(!request.client_capabilities.fs.write_text_file);
        assert!(!request.client_capabilities.terminal);
        assert_eq!(
            request.client_info.as_ref().map(|info| info.name.as_str()),
            Some("OxideTerm")
        );
    }

    #[test]
    fn permission_projection_and_decision_preserve_option_ids() {
        let request = RequestPermissionRequest::new(
            "session-1",
            ToolCallUpdate::new(
                "tool-1",
                ToolCallUpdateFields::new()
                    .title("Run command")
                    .raw_input(serde_json::json!({ "command": "pwd" })),
            ),
            vec![
                PermissionOption::new("allow-once", "Allow", PermissionOptionKind::AllowOnce),
                PermissionOption::new("reject-once", "Reject", PermissionOptionKind::RejectOnce),
            ],
        );

        let projection = acp_permission_request_projection(&request);
        assert_eq!(projection.tool_call_id, "tool-1");
        assert_eq!(projection.name, "Run command");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&projection.arguments)
                .expect("permission arguments"),
            serde_json::json!({ "input": { "command": "pwd" } })
        );

        assert!(matches!(
            acp_permission_response_for_decision(&request, true).outcome,
            RequestPermissionOutcome::Selected(selected)
                if selected.option_id.to_string() == "allow-once"
        ));
        assert!(matches!(
            acp_permission_response_for_decision(&request, false).outcome,
            RequestPermissionOutcome::Selected(selected)
                if selected.option_id.to_string() == "reject-once"
        ));
    }

    #[tokio::test]
    async fn read_text_file_enforces_root_and_line_range() {
        let root = tempfile::tempdir().expect("root tempdir");
        let file_path = root.path().join("file.txt");
        tokio::fs::write(&file_path, "one\ntwo\nthree\n")
            .await
            .expect("write fixture");
        let response = resolve_acp_read_text_file_request(
            root.path(),
            &ReadTextFileRequest::new("session-1", file_path.clone())
                .line(Some(2))
                .limit(Some(1)),
        )
        .await
        .expect("read response");
        assert_eq!(response.content, "two");

        let outside = tempfile::NamedTempFile::new().expect("outside temp file");
        let error = resolve_acp_read_text_file_request(
            root.path(),
            &ReadTextFileRequest::new("session-1", outside.path()),
        )
        .await
        .expect_err("path outside root is rejected");
        assert!(error.to_string().contains("outside the session root"));
    }

    #[tokio::test]
    async fn write_text_file_enforces_root_for_new_and_existing_targets() {
        let root = tempfile::tempdir().expect("root tempdir");
        let file_path = root.path().join("new.txt");
        resolve_acp_write_text_file_request(
            root.path(),
            &WriteTextFileRequest::new("session-1", file_path.clone(), "written"),
        )
        .await
        .expect("write response");
        assert_eq!(
            tokio::fs::read_to_string(&file_path)
                .await
                .expect("written file"),
            "written"
        );

        let outside = tempfile::NamedTempFile::new().expect("outside temp file");
        let error = resolve_acp_write_text_file_request(
            root.path(),
            &WriteTextFileRequest::new("session-1", outside.path(), "blocked"),
        )
        .await
        .expect_err("path outside root is rejected");
        assert!(error.to_string().contains("outside the session root"));
    }

    #[tokio::test]
    async fn terminal_registry_runs_command_and_reports_output() {
        let root = tempfile::tempdir().expect("root tempdir");
        let registry = AcpTerminalRegistry::new();
        let create = registry
            .create_terminal(
                root.path(),
                &CreateTerminalRequest::new("session-1", "rustc")
                    .args(vec!["--version".to_string()])
                    .output_byte_limit(Some(256)),
            )
            .await
            .expect("create terminal");
        let terminal_id = create.terminal_id.to_string();
        let exit = registry
            .wait_for_terminal_exit(&WaitForTerminalExitRequest::new(
                "session-1",
                terminal_id.clone(),
            ))
            .await
            .expect("wait terminal");
        assert_eq!(exit.exit_status.exit_code, Some(0));

        let output = registry
            .terminal_output(&TerminalOutputRequest::new(
                "session-1",
                terminal_id.clone(),
            ))
            .await
            .expect("terminal output");
        assert!(output.output.contains("rustc"));

        registry
            .release_terminal(&ReleaseTerminalRequest::new("session-1", terminal_id))
            .await
            .expect("release terminal");
    }

    #[tokio::test]
    async fn initialize_agent_sends_v1_request_to_sdk_agent() {
        let fake_agent = Agent.builder().on_receive_request(
            async move |request: InitializeRequest, responder, _connection| {
                assert_eq!(request.protocol_version, ProtocolVersion::V1);
                assert!(!request.client_capabilities.fs.read_text_file);
                assert!(!request.client_capabilities.fs.write_text_file);
                assert!(!request.client_capabilities.terminal);
                responder.respond(
                    InitializeResponse::new(request.protocol_version)
                        .agent_capabilities(AgentCapabilities::new()),
                )
            },
            agent_client_protocol::on_receive_request!(),
        );

        let response = initialize_acp_agent(
            fake_agent,
            "2.0.0-test".to_string(),
            AcpHostCapabilityPolicy::default(),
        )
        .await
        .expect("initialize response");

        assert_eq!(response.protocol_version, ProtocolVersion::V1);
    }

    #[tokio::test]
    async fn initialize_agent_reports_missing_binary() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let mut config = launch_config();
        config.command = temp_dir
            .path()
            .join("missing-acp-agent")
            .to_string_lossy()
            .into_owned();
        config.args.clear();
        config.env.clear();
        let launcher = build_acp_stdio_launcher(config).expect("launcher");

        let error = initialize_acp_agent(
            launcher,
            "2.0.0-test".to_string(),
            AcpHostCapabilityPolicy::default(),
        )
        .await
        .expect_err("missing binary should fail initialize");

        assert_eq!(
            error.code,
            agent_client_protocol::schema::ErrorCode::InternalError
        );
    }

    #[tokio::test]
    async fn initialize_agent_rejects_unsupported_protocol_version() {
        let fake_agent = Agent.builder().on_receive_request(
            async move |_request: InitializeRequest, responder, _connection| {
                responder.respond(
                    InitializeResponse::new(ProtocolVersion::V0)
                        .agent_capabilities(AgentCapabilities::new()),
                )
            },
            agent_client_protocol::on_receive_request!(),
        );

        let error = initialize_acp_agent(
            fake_agent,
            "2.0.0-test".to_string(),
            AcpHostCapabilityPolicy::default(),
        )
        .await
        .expect_err("unsupported protocol version should fail initialize");

        assert_eq!(
            error.code,
            agent_client_protocol::schema::ErrorCode::InternalError
        );
    }

    #[tokio::test]
    async fn runtime_rejects_unadvertised_optional_lifecycle_methods() {
        let fake_agent = Agent.builder().on_receive_request(
            async move |request: InitializeRequest, responder, _connection| {
                responder.respond(
                    InitializeResponse::new(request.protocol_version)
                        .agent_capabilities(AgentCapabilities::new()),
                )
            },
            agent_client_protocol::on_receive_request!(),
        );

        let error = with_acp_agent_runtime(
            fake_agent,
            "2.0.0-test".to_string(),
            AcpHostCapabilityPolicy::default(),
            async |runtime| runtime.close_session("session-1").await,
        )
        .await
        .expect_err("close requires advertised capability");

        assert_eq!(
            error.code,
            agent_client_protocol::schema::ErrorCode::MethodNotFound
        );
    }

    #[tokio::test]
    async fn session_config_discovery_starts_session_without_sending_prompt() {
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
                async move |request: NewSessionRequest, responder, _connection| {
                    assert_eq!(request.cwd, PathBuf::from("/workspace"));
                    responder.respond(NewSessionResponse::new("discovery-session").config_options(
                        vec![SessionConfigOption::select(
                            "model",
                            "Model",
                            "gpt-5.6-sol",
                            vec![SessionConfigSelectOption::new(
                                "gpt-5.6-sol",
                                "gpt-5.6-sol",
                            )],
                        )
                        .category(SessionConfigOptionCategory::Model)],
                    ))
                },
                agent_client_protocol::on_receive_request!(),
            );

        let options = discover_acp_session_config_options(
            fake_agent,
            "2.0.0-test".to_string(),
            AcpHostCapabilityPolicy::default(),
            PathBuf::from("/workspace"),
        )
        .await
        .expect("session config discovery");

        assert_eq!(options.len(), 1);
        assert_eq!(options[0].category.as_deref(), Some("model"));
        assert_eq!(options[0].current_value_id, "gpt-5.6-sol");
    }

    #[tokio::test]
    async fn runtime_runs_initialize_auth_session_prompt_cancel_close_logout() {
        let fake_agent = Agent
            .builder()
            .on_receive_request(
                async move |request: InitializeRequest, responder, _connection| {
                    let capabilities = AgentCapabilities::new()
                        .auth(AgentAuthCapabilities::new().logout(LogoutCapabilities::new()))
                        .session_capabilities(
                            SessionCapabilities::new().close(SessionCloseCapabilities::new()),
                        );
                    responder.respond(
                        InitializeResponse::new(request.protocol_version)
                            .agent_capabilities(capabilities)
                            .auth_methods(vec![AuthMethod::Agent(AuthMethodAgent::new(
                                "agent-auth",
                                "Agent Auth",
                            ))]),
                    )
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |request: AuthenticateRequest, responder, _connection| {
                    assert_eq!(request.method_id.to_string(), "agent-auth");
                    responder.respond(AuthenticateResponse::new())
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |request: NewSessionRequest, responder, _connection| {
                    assert_eq!(request.cwd, PathBuf::from("/workspace"));
                    responder.respond(NewSessionResponse::new("session-1").config_options(vec![
                        SessionConfigOption::select(
                            "model",
                            "Model",
                            "model-a",
                            vec![SessionConfigSelectOption::new("model-a", "Model A")],
                        )
                        .category(SessionConfigOptionCategory::Model),
                    ]))
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |request: PromptRequest, responder, _connection| {
                    assert_eq!(request.session_id.to_string(), "session-1");
                    responder.respond(PromptResponse::new(StopReason::EndTurn))
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_notification(
                async move |notification: CancelNotification, _connection| {
                    assert_eq!(notification.session_id.to_string(), "session-1");
                    Ok(())
                },
                agent_client_protocol::on_receive_notification!(),
            )
            .on_receive_request(
                async move |request: CloseSessionRequest, responder, _connection| {
                    assert_eq!(request.session_id.to_string(), "session-1");
                    responder.respond(CloseSessionResponse::new())
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |_request: LogoutRequest, responder, _connection| {
                    responder.respond(LogoutResponse::new())
                },
                agent_client_protocol::on_receive_request!(),
            );

        with_acp_agent_runtime(
            fake_agent,
            "2.0.0-test".to_string(),
            AcpHostCapabilityPolicy::default(),
            async |runtime| {
                runtime.authenticate("agent-auth").await?;
                let mut session = runtime
                    .start_session(NewSessionRequest::new(PathBuf::from("/workspace")))
                    .await?;
                assert_eq!(session.config_options().len(), 1);
                session.send_prompt("hello")?;
                runtime.cancel_session(session.session_id().clone())?;
                runtime.close_session(session.session_id().clone()).await?;
                runtime.logout().await?;
                Ok(())
            },
        )
        .await
        .expect("runtime lifecycle");
    }

    #[tokio::test]
    async fn prompt_runtime_resumes_existing_session_id() {
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
                    assert_eq!(request.session_id.to_string(), "session-existing");
                    assert_eq!(request.cwd, PathBuf::from("/workspace"));
                    assert_eq!(request.mcp_servers.len(), 1);
                    let McpServer::Stdio(server) = &request.mcp_servers[0] else {
                        panic!("expected test MCP stdio server");
                    };
                    assert_eq!(server.name, "OxideTerm Host Tools");
                    responder.respond(ResumeSessionResponse::new())
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |request: PromptRequest, responder, _connection| {
                    assert_eq!(request.session_id.to_string(), "session-existing");
                    responder.respond(PromptResponse::new(StopReason::EndTurn))
                },
                agent_client_protocol::on_receive_request!(),
            );
        let (event_tx, _event_rx) = mpsc::unbounded_channel();

        let outcome = run_acp_prompt_session_events(
            fake_agent,
            "2.0.0-test".to_string(),
            AcpHostCapabilityPolicy::default(),
            PathBuf::from("/workspace"),
            Some("session-existing".to_string()),
            None,
            vec![McpServer::Stdio(McpServerStdio::new(
                "OxideTerm Host Tools",
                "test-mcp-server",
            ))],
            "hello".to_string(),
            event_tx,
            AcpRuntimeRegistry::new(),
            "conversation-1".to_string(),
            "generation-1".to_string(),
        )
        .await
        .expect("resume existing session");

        assert_eq!(outcome.session_id, "session-existing");
        assert_eq!(outcome.session_metadata, None);
    }

    #[tokio::test]
    async fn prompt_runtime_replays_session_model_before_prompt() {
        let selection_applied = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let selected_for_set = Arc::clone(&selection_applied);
        let selected_for_prompt = Arc::clone(&selection_applied);
        let model_options = || {
            vec![
                SessionConfigOption::select(
                    "model",
                    "Model",
                    "model-b",
                    vec![
                        SessionConfigSelectOption::new("model-a", "Model A"),
                        SessionConfigSelectOption::new("model-b", "Model B"),
                    ],
                )
                .category(SessionConfigOptionCategory::Model),
            ]
        };
        let initial_model_options = vec![
            SessionConfigOption::select(
                "model",
                "Model",
                "model-a",
                vec![
                    SessionConfigSelectOption::new("model-a", "Model A"),
                    SessionConfigSelectOption::new("model-b", "Model B"),
                ],
            )
            .category(SessionConfigOptionCategory::Model),
        ];
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
                    responder.respond(
                        NewSessionResponse::new("session-1")
                            .config_options(initial_model_options.clone()),
                    )
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |request: SetSessionConfigOptionRequest, responder, _connection| {
                    assert_eq!(request.config_id.to_string(), "model");
                    assert_eq!(request.value.to_string(), "model-b");
                    selected_for_set.store(true, Ordering::SeqCst);
                    responder.respond(SetSessionConfigOptionResponse::new(model_options()))
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |_request: PromptRequest, responder, connection| {
                    assert!(selected_for_prompt.load(Ordering::SeqCst));
                    connection.send_notification(SessionNotification::new(
                        "session-1",
                        SessionUpdate::ConfigOptionUpdate(
                            agent_client_protocol::schema::ConfigOptionUpdate::new(vec![
                                SessionConfigOption::select(
                                    "model",
                                    "Model",
                                    "model-a",
                                    vec![
                                        SessionConfigSelectOption::new("model-a", "Model A"),
                                        SessionConfigSelectOption::new("model-b", "Model B"),
                                    ],
                                )
                                .category(SessionConfigOptionCategory::Model),
                            ]),
                        ),
                    ))?;
                    responder.respond(PromptResponse::new(StopReason::EndTurn))
                },
                agent_client_protocol::on_receive_request!(),
            );
        let (event_tx, _event_rx) = mpsc::unbounded_channel();

        let outcome = run_acp_prompt_session_events(
            fake_agent,
            "2.0.0-test".to_string(),
            AcpHostCapabilityPolicy::default(),
            PathBuf::from("/workspace"),
            None,
            Some(AcpSessionConfigSelection {
                config_id: "model".to_string(),
                value_id: "model-b".to_string(),
            }),
            Vec::new(),
            "hello".to_string(),
            event_tx,
            AcpRuntimeRegistry::new(),
            "conversation-1".to_string(),
            "generation-1".to_string(),
        )
        .await
        .expect("session model selection");

        assert_eq!(
            outcome.session_config_options[0].current_value_id,
            "model-a"
        );
    }

    #[tokio::test]
    async fn runtime_events_forward_client_requests_to_channel() {
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
                    responder.respond(NewSessionResponse::new("session-1"))
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |_request: PromptRequest, responder, connection| {
                    // ACP handlers must not synchronously wait on reverse requests;
                    // the SDK event loop cannot process the client response until
                    // this handler yields.
                    let request_connection = connection.clone();
                    connection.spawn(async move {
                        let file = request_connection
                            .send_request(ReadTextFileRequest::new(
                                "session-1",
                                "/workspace/file.txt",
                            ))
                            .block_task()
                            .await?;
                        assert_eq!(file.content, "from-host");
                        Ok(())
                    })?;
                    responder.respond(PromptResponse::new(StopReason::EndTurn))
                },
                agent_client_protocol::on_receive_request!(),
            );
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();

        with_acp_agent_runtime_events(
            fake_agent,
            "2.0.0-test".to_string(),
            AcpHostCapabilityPolicy {
                fs_read_text_file: true,
                fs_write_text_file: false,
                terminal: false,
            },
            event_tx,
            async move |runtime| {
                let mut session = runtime
                    .start_session(NewSessionRequest::new(PathBuf::from("/workspace")))
                    .await?;
                session.send_prompt("hello")?;
                match event_rx.recv().await.expect("client event") {
                    AcpClientEvent::ReadTextFile {
                        request,
                        response_tx,
                    } => {
                        assert_eq!(request.session_id.to_string(), "session-1");
                        assert_eq!(request.path, PathBuf::from("/workspace/file.txt"));
                        response_tx
                            .send(Ok(ReadTextFileResponse::new("from-host")))
                            .expect("send read response");
                    }
                    _ => panic!("unexpected client event"),
                }
                match session.read_update().await? {
                    agent_client_protocol::SessionMessage::StopReason(StopReason::EndTurn) => {}
                    _ => panic!("unexpected session update"),
                }
                Ok(())
            },
        )
        .await
        .expect("runtime with client events");
    }

    #[tokio::test]
    async fn runtime_registry_cancel_uses_registered_session_connection() {
        let (cancel_seen_tx, cancel_seen_rx) = oneshot::channel();
        let cancel_seen_tx = Arc::new(parking_lot::Mutex::new(Some(cancel_seen_tx)));
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
                    responder.respond(NewSessionResponse::new("session-1"))
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_notification(
                {
                    let cancel_seen_tx = cancel_seen_tx.clone();
                    async move |notification: CancelNotification, _connection| {
                        assert_eq!(notification.session_id.to_string(), "session-1");
                        if let Some(sender) = cancel_seen_tx.lock().take() {
                            let _ = sender.send(());
                        }
                        Ok(())
                    }
                },
                agent_client_protocol::on_receive_notification!(),
            );
        let registry = AcpRuntimeRegistry::new();
        let key = AcpRuntimeHandleKey::new("conversation-1", "generation-1", "session-1");

        with_acp_agent_runtime(
            fake_agent,
            "2.0.0-test".to_string(),
            AcpHostCapabilityPolicy::default(),
            async |runtime| {
                let session = runtime
                    .start_session(NewSessionRequest::new(PathBuf::from("/workspace")))
                    .await?;
                let guard = runtime.register_session_handle(registry.clone(), key.clone())?;
                assert!(registry.contains(&key));
                registry.cancel_generation("conversation-1", "generation-1")?;
                cancel_seen_rx
                    .await
                    .map_err(agent_client_protocol::Error::into_internal_error)?;
                drop(session);
                drop(guard);
                assert!(!registry.contains(&key));
                Ok(())
            },
        )
        .await
        .expect("registry cancel");
    }

    #[test]
    fn session_update_text_chunks_map_to_ai_stream_events() {
        let content = SessionNotification::new(
            "session-1",
            SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::from("hello"))),
        );
        let thinking = SessionNotification::new(
            "session-1",
            SessionUpdate::AgentThoughtChunk(ContentChunk::new(ContentBlock::from("thinking"))),
        );

        assert_eq!(
            acp_session_notification_to_ai_stream_events(&content),
            vec![AiStreamEvent::Content("hello".to_string())]
        );
        assert_eq!(
            acp_session_notification_to_ai_stream_events(&thinking),
            vec![AiStreamEvent::Thinking("thinking".to_string())]
        );
    }

    #[test]
    fn session_update_tool_calls_map_to_ai_stream_events() {
        let tool_call = ToolCall::new("tool-1", "Read file")
            .status(ToolCallStatus::InProgress)
            .raw_input(serde_json::json!({"path": "/workspace/file.txt"}));
        let notification =
            SessionNotification::new("session-1", SessionUpdate::ToolCall(tool_call));

        let events = acp_session_notification_to_ai_stream_events(&notification);

        assert_eq!(events.len(), 1);
        let AiStreamEvent::ToolCall {
            id,
            name,
            arguments,
        } = &events[0]
        else {
            panic!("tool call event");
        };
        assert_eq!(id, "tool-1");
        assert_eq!(name, "Read file");
        assert!(arguments.contains("/workspace/file.txt"));
        assert!(arguments.contains("in_progress"));
    }

    #[test]
    fn session_update_completed_tool_update_maps_to_complete_event() {
        let update = ToolCallUpdate::new(
            "tool-1",
            ToolCallUpdateFields::new()
                .title("Read file".to_string())
                .status(ToolCallStatus::Completed)
                .raw_output(serde_json::json!({"ok": true})),
        );
        let notification =
            SessionNotification::new("session-1", SessionUpdate::ToolCallUpdate(update));

        let events = acp_session_notification_to_ai_stream_events(&notification);

        assert_eq!(events.len(), 1);
        let AiStreamEvent::ToolCallComplete {
            id,
            name,
            arguments,
        } = &events[0]
        else {
            panic!("tool call complete event");
        };
        assert_eq!(id, "tool-1");
        assert_eq!(name, "Read file");
        assert!(arguments.contains("completed"));
        assert!(arguments.contains("\"ok\":true"));
    }

    #[tokio::test]
    async fn client_event_conversion_rejects_unwired_host_requests() {
        let (response_tx, response_rx) = oneshot::channel();
        let events = acp_client_event_to_ai_stream_events(AcpClientEvent::ReadTextFile {
            request: ReadTextFileRequest::new("session-1", "/workspace/file.txt"),
            response_tx,
        });

        assert!(events.is_empty());
        let error = response_rx
            .await
            .expect("response sent")
            .expect_err("host request rejected");
        assert_eq!(
            error.code,
            agent_client_protocol::schema::ErrorCode::MethodNotFound
        );
    }
}
