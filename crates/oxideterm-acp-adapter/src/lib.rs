// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! ACP stdio adapter entrypoint for local agent CLIs that do not expose ACP directly.

use std::{
    collections::HashMap,
    ffi::OsString,
    path::PathBuf,
    process::Stdio as ProcessStdio,
    sync::{Arc, Mutex},
};

use agent_client_protocol::{
    Agent, Client, ConnectionTo, Dispatch, Stdio,
    schema::{
        AgentCapabilities, CancelNotification, CloseSessionRequest, CloseSessionResponse,
        ContentBlock, ContentChunk, DeleteSessionRequest, DeleteSessionResponse, Implementation,
        InitializeRequest, InitializeResponse, NewSessionRequest, NewSessionResponse,
        PermissionOption, PermissionOptionKind, PromptRequest, PromptResponse, ProtocolVersion,
        RequestPermissionOutcome, RequestPermissionRequest, SessionId, SessionNotification,
        SessionUpdate, StopReason, ToolCall, ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields,
        ToolKind,
    },
};
use clap::{Parser, ValueEnum};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::{mpsc, oneshot},
    time::{Duration, timeout},
};
use uuid::Uuid;

pub const ACP_ADAPTER_ARG: &str = "--acp-adapter";

#[derive(Debug, Parser)]
#[command(name = "oxideterm --acp-adapter")]
#[command(about = "Bridge local coding-agent CLIs to ACP stdio.")]
struct Cli {
    #[arg(value_enum)]
    provider: AdapterProvider,

    #[arg(long)]
    command: Option<String>,

    #[arg(long = "arg")]
    extra_args: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum AdapterProvider {
    ClaudeCode,
    Codex,
}

#[derive(Clone, Debug)]
struct AdapterConfig {
    provider: AdapterProvider,
    command: String,
    extra_args: Vec<String>,
}

#[derive(Clone, Debug)]
struct SessionState {
    cwd: PathBuf,
    claude_session_id: Option<String>,
    codex_thread_id: Option<String>,
}

type Sessions = Arc<Mutex<HashMap<SessionId, SessionState>>>;
type ActiveRuns = Arc<Mutex<HashMap<SessionId, ActiveRun>>>;

struct ProviderOutcome {
    stop_reason: StopReason,
    claude_session_id: Option<String>,
    codex_thread_id: Option<String>,
}

#[derive(Clone)]
struct ActiveRun {
    run_id: Uuid,
    cancel_tx: mpsc::UnboundedSender<()>,
}

pub fn run_from_env_if_requested() {
    let mut args = std::env::args_os();
    let _program = args.next();
    if args.next().as_deref() != Some(std::ffi::OsStr::new(ACP_ADAPTER_ARG)) {
        return;
    }

    // The adapter path must finish before GPUI initializes so stdout stays a
    // clean ACP transport and no singleton UI lock is acquired by child agents.
    let cli_args = std::iter::once(OsString::from("oxideterm-acp-adapter")).chain(args);
    let cli = Cli::try_parse_from(cli_args).unwrap_or_else(|error| error.exit());
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap_or_else(|error| {
            eprintln!("failed to start ACP adapter runtime: {error}");
            std::process::exit(1);
        });
    let exit_code = match runtime.block_on(run_adapter(cli)) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("ACP adapter failed: {error}");
            1
        }
    };
    std::process::exit(exit_code);
}

async fn run_adapter(cli: Cli) -> agent_client_protocol::Result<()> {
    let config = Arc::new(AdapterConfig::from_cli(cli));
    let sessions = Sessions::default();
    let active_runs = ActiveRuns::default();

    Agent
        .builder()
        .name("oxideterm-acp-adapter")
        .on_receive_request(
            {
                let config = Arc::clone(&config);
                async move |initialize: InitializeRequest, responder, _connection| {
                    let protocol_version = supported_protocol_version(initialize.protocol_version);
                    responder.respond(
                        InitializeResponse::new(protocol_version)
                            .agent_capabilities(AgentCapabilities::new())
                            .agent_info(Implementation::new(
                                config.provider.agent_name(),
                                env!("CARGO_PKG_VERSION"),
                            )),
                    )
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let sessions = Arc::clone(&sessions);
                async move |request: NewSessionRequest, responder, _connection| {
                    let session_id = SessionId::new(format!(
                        "oxideterm-{}-{}",
                        env!("CARGO_PKG_VERSION"),
                        Uuid::new_v4()
                    ));
                    // Store only the session root needed to launch the wrapped CLI.
                    sessions.lock().expect("session registry lock").insert(
                        session_id.clone(),
                        SessionState {
                            cwd: request.cwd,
                            claude_session_id: None,
                            codex_thread_id: None,
                        },
                    );
                    responder.respond(NewSessionResponse::new(session_id))
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let config = Arc::clone(&config);
                let sessions = Arc::clone(&sessions);
                let active_runs = Arc::clone(&active_runs);
                async move |request: PromptRequest, responder, connection: ConnectionTo<Client>| {
                    match handle_prompt(&config, &sessions, &active_runs, request, connection).await
                    {
                        Ok(response) => responder.respond(response),
                        Err(error) => responder.respond_with_error(error),
                    }
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let sessions = Arc::clone(&sessions);
                let active_runs = Arc::clone(&active_runs);
                async move |request: CloseSessionRequest, responder, _connection| {
                    cancel_active_run(&active_runs, &request.session_id);
                    sessions
                        .lock()
                        .expect("session registry lock")
                        .remove(&request.session_id);
                    responder.respond(CloseSessionResponse::new())
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let sessions = Arc::clone(&sessions);
                let active_runs = Arc::clone(&active_runs);
                async move |request: DeleteSessionRequest, responder, _connection| {
                    cancel_active_run(&active_runs, &request.session_id);
                    sessions
                        .lock()
                        .expect("session registry lock")
                        .remove(&request.session_id);
                    responder.respond(DeleteSessionResponse::new())
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_notification(
            {
                let active_runs = Arc::clone(&active_runs);
                async move |cancel: CancelNotification, _connection| {
                    cancel_active_run(&active_runs, &cancel.session_id);
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_dispatch(
            async move |message: Dispatch, connection: ConnectionTo<Client>| {
                message.respond_with_error(
                    agent_client_protocol::Error::method_not_found()
                        .data("oxideterm-acp-adapter unsupported ACP method"),
                    connection,
                )
            },
            agent_client_protocol::on_receive_dispatch!(),
        )
        .connect_to(Stdio::new())
        .await
}

fn cancel_active_run(active_runs: &ActiveRuns, session_id: &SessionId) {
    if let Some(run) = active_runs
        .lock()
        .expect("active ACP run lock")
        .remove(session_id)
    {
        // The prompt task owns the child process; the channel keeps cancellation
        // routed through that owner so process cleanup remains single-threaded.
        let _ = run.cancel_tx.send(());
    }
}

fn cleanup_active_run(active_runs: &ActiveRuns, session_id: &SessionId, run_id: Uuid) {
    let mut runs = active_runs.lock().expect("active ACP run lock");
    if runs.get(session_id).is_some_and(|run| run.run_id == run_id) {
        runs.remove(session_id);
    }
}

impl AdapterConfig {
    fn from_cli(cli: Cli) -> Self {
        let command = cli
            .command
            .unwrap_or_else(|| cli.provider.default_command().to_string());
        Self {
            provider: cli.provider,
            command,
            extra_args: cli.extra_args,
        }
    }
}

impl AdapterProvider {
    fn default_command(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude",
            Self::Codex => "codex",
        }
    }

    fn agent_name(self) -> &'static str {
        match self {
            Self::ClaudeCode => "OxideTerm Claude Code ACP Adapter",
            Self::Codex => "OxideTerm Codex ACP Adapter",
        }
    }
}

fn supported_protocol_version(version: ProtocolVersion) -> ProtocolVersion {
    match version {
        ProtocolVersion::V1 => ProtocolVersion::V1,
        _ => ProtocolVersion::V1,
    }
}

async fn handle_prompt(
    config: &AdapterConfig,
    sessions: &Sessions,
    active_runs: &ActiveRuns,
    request: PromptRequest,
    connection: ConnectionTo<Client>,
) -> Result<PromptResponse, agent_client_protocol::Error> {
    let session = sessions
        .lock()
        .expect("session registry lock")
        .get(&request.session_id)
        .cloned()
        .ok_or_else(|| agent_client_protocol::util::internal_error("ACP session was not found"))?;
    let prompt = prompt_text(&request.prompt);
    if prompt.trim().is_empty() {
        return Err(agent_client_protocol::util::internal_error(
            "ACP prompt did not contain text content",
        ));
    }

    let outcome = stream_provider(
        config,
        active_runs,
        request.session_id.clone(),
        session,
        prompt,
        connection,
    )
    .await?;
    if let Some(codex_thread_id) = outcome.codex_thread_id.clone()
        && let Some(session) = sessions
            .lock()
            .expect("session registry lock")
            .get_mut(&request.session_id)
    {
        session.codex_thread_id = Some(codex_thread_id);
    }
    if let Some(claude_session_id) = outcome.claude_session_id.clone()
        && let Some(session) = sessions
            .lock()
            .expect("session registry lock")
            .get_mut(&request.session_id)
    {
        session.claude_session_id = Some(claude_session_id);
    }
    Ok(PromptResponse::new(outcome.stop_reason))
}

async fn stream_provider(
    config: &AdapterConfig,
    active_runs: &ActiveRuns,
    session_id: SessionId,
    session: SessionState,
    prompt: String,
    connection: ConnectionTo<Client>,
) -> Result<ProviderOutcome, agent_client_protocol::Error> {
    match config.provider {
        AdapterProvider::ClaudeCode => {
            stream_claude_code_provider(
                config,
                active_runs,
                session_id,
                session.cwd,
                session.claude_session_id,
                prompt,
                connection,
            )
            .await
        }
        AdapterProvider::Codex => {
            stream_codex_app_server_provider(
                config,
                active_runs,
                session_id,
                session.cwd,
                session.codex_thread_id,
                prompt,
                connection,
            )
            .await
        }
    }
}

async fn stream_claude_code_provider(
    config: &AdapterConfig,
    active_runs: &ActiveRuns,
    session_id: SessionId,
    cwd: PathBuf,
    previous_session_id: Option<String>,
    prompt: String,
    connection: ConnectionTo<Client>,
) -> Result<ProviderOutcome, agent_client_protocol::Error> {
    let mut command = Command::new(config.command.trim());
    command.current_dir(cwd);
    command.args([
        "-p",
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
    ]);
    if let Some(previous_session_id) = previous_session_id {
        command.args(["--resume", previous_session_id.as_str()]);
    }
    command.args(&config.extra_args);
    command.arg(prompt);
    command.kill_on_drop(true);
    command.stdin(ProcessStdio::null());
    command.stderr(ProcessStdio::piped());
    command.stdout(ProcessStdio::piped());

    let mut child = command
        .spawn()
        .map_err(agent_client_protocol::Error::into_internal_error)?;
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(async move {
            let mut stderr = stderr;
            let mut sink = tokio::io::sink();
            let _ = tokio::io::copy(&mut stderr, &mut sink).await;
        });
    }
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| agent_client_protocol::util::internal_error("provider stdout missing"))?;

    let run_id = Uuid::new_v4();
    let (cancel_tx, cancel_rx) = mpsc::unbounded_channel();
    {
        let previous = active_runs
            .lock()
            .expect("active ACP run lock")
            .insert(session_id.clone(), ActiveRun { run_id, cancel_tx });
        if let Some(previous) = previous {
            // A new prompt supersedes the previous process for the same ACP session.
            let _ = previous.cancel_tx.send(());
        }
    }

    let outcome = read_claude_stream_json_stdout(
        config,
        &session_id,
        &connection,
        &mut child,
        stdout,
        cancel_rx,
    )
    .await;
    cleanup_active_run(active_runs, &session_id, run_id);
    outcome
}

async fn stream_codex_app_server_provider(
    config: &AdapterConfig,
    active_runs: &ActiveRuns,
    session_id: SessionId,
    cwd: PathBuf,
    previous_thread_id: Option<String>,
    prompt: String,
    connection: ConnectionTo<Client>,
) -> Result<ProviderOutcome, agent_client_protocol::Error> {
    let mut command = Command::new(config.command.trim());
    command.current_dir(&cwd);
    command.args(["app-server", "--stdio"]);
    command.args(&config.extra_args);
    command.kill_on_drop(true);
    command.stdin(ProcessStdio::piped());
    command.stderr(ProcessStdio::piped());
    command.stdout(ProcessStdio::piped());

    let mut child = command
        .spawn()
        .map_err(agent_client_protocol::Error::into_internal_error)?;
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(async move {
            let mut stderr = stderr;
            let mut sink = tokio::io::sink();
            let _ = tokio::io::copy(&mut stderr, &mut sink).await;
        });
    }
    let stdin = child.stdin.take().ok_or_else(|| {
        agent_client_protocol::util::internal_error("codex app-server stdin missing")
    })?;
    let stdout = child.stdout.take().ok_or_else(|| {
        agent_client_protocol::util::internal_error("codex app-server stdout missing")
    })?;
    let mut client = CodexAppServerClient {
        stdin,
        stdout: BufReader::new(stdout),
        next_id: 1,
    };

    let run_id = Uuid::new_v4();
    let (cancel_tx, cancel_rx) = mpsc::unbounded_channel();
    {
        let previous = active_runs
            .lock()
            .expect("active ACP run lock")
            .insert(session_id.clone(), ActiveRun { run_id, cancel_tx });
        if let Some(previous) = previous {
            // A new prompt supersedes the previous process for the same ACP session.
            let _ = previous.cancel_tx.send(());
        }
    }

    let outcome = run_codex_app_server_turn(
        &mut client,
        &mut child,
        &session_id,
        &connection,
        &cwd,
        previous_thread_id,
        prompt,
        cancel_rx,
    )
    .await;
    cleanup_active_run(active_runs, &session_id, run_id);
    outcome
}

struct CodexAppServerClient {
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl CodexAppServerClient {
    async fn send_request(
        &mut self,
        method: &str,
        params: Value,
    ) -> Result<u64, agent_client_protocol::Error> {
        let id = self.next_id;
        self.next_id += 1;
        self.send_json(json!({
            "id": id,
            "method": method,
            "params": params,
        }))
        .await?;
        Ok(id)
    }

    async fn send_notification(
        &mut self,
        method: &str,
        params: Value,
    ) -> Result<(), agent_client_protocol::Error> {
        self.send_json(json!({
            "method": method,
            "params": params,
        }))
        .await
    }

    async fn send_response(
        &mut self,
        id: Value,
        result: Value,
    ) -> Result<(), agent_client_protocol::Error> {
        self.send_json(json!({
            "id": id,
            "result": result,
        }))
        .await
    }

    async fn send_error_response(
        &mut self,
        id: Value,
        message: &str,
    ) -> Result<(), agent_client_protocol::Error> {
        self.send_json(json!({
            "id": id,
            "error": {
                "code": -32601,
                "message": message,
            },
        }))
        .await
    }

    async fn send_json(&mut self, value: Value) -> Result<(), agent_client_protocol::Error> {
        let mut line = serde_json::to_vec(&value)
            .map_err(agent_client_protocol::Error::into_internal_error)?;
        line.push(b'\n');
        self.stdin
            .write_all(&line)
            .await
            .map_err(agent_client_protocol::Error::into_internal_error)
    }

    async fn read_json(&mut self) -> Result<Option<Value>, agent_client_protocol::Error> {
        let mut line = String::new();
        let read_len = self
            .stdout
            .read_line(&mut line)
            .await
            .map_err(agent_client_protocol::Error::into_internal_error)?;
        if read_len == 0 {
            return Ok(None);
        }
        let value = serde_json::from_str(line.trim_end())
            .map_err(agent_client_protocol::Error::into_internal_error)?;
        Ok(Some(value))
    }
}

async fn run_codex_app_server_turn(
    client: &mut CodexAppServerClient,
    child: &mut Child,
    session_id: &SessionId,
    connection: &ConnectionTo<Client>,
    cwd: &PathBuf,
    previous_thread_id: Option<String>,
    prompt: String,
    cancel_rx: mpsc::UnboundedReceiver<()>,
) -> Result<ProviderOutcome, agent_client_protocol::Error> {
    let initialize_id = client
        .send_request(
            "initialize",
            json!({
                "clientInfo": {
                    "name": "oxideterm",
                    "title": "OxideTerm",
                    "version": env!("CARGO_PKG_VERSION"),
                },
                "capabilities": {
                    "experimentalApi": true,
                },
            }),
        )
        .await?;
    wait_for_app_server_response(client, initialize_id, session_id, connection).await?;
    client.send_notification("initialized", json!({})).await?;

    let (thread_id, used_existing_thread) =
        start_or_resume_codex_thread(client, session_id, connection, cwd, previous_thread_id)
            .await?;
    let turn_id = client
        .send_request(
            "turn/start",
            json!({
                "threadId": thread_id,
                "cwd": cwd,
                "input": [{
                    "type": "text",
                    "text": prompt,
                }],
            }),
        )
        .await?;
    let turn_response =
        wait_for_app_server_response(client, turn_id, session_id, connection).await?;
    let codex_turn_id = turn_response
        .get("turn")
        .and_then(|turn| turn.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string);

    let stop_reason = read_codex_turn_notifications(
        client,
        child,
        session_id,
        connection,
        &thread_id,
        codex_turn_id,
        cancel_rx,
    )
    .await?;
    Ok(ProviderOutcome {
        stop_reason,
        claude_session_id: None,
        codex_thread_id: if used_existing_thread || matches!(stop_reason, StopReason::EndTurn) {
            Some(thread_id)
        } else {
            None
        },
    })
}

async fn start_or_resume_codex_thread(
    client: &mut CodexAppServerClient,
    session_id: &SessionId,
    connection: &ConnectionTo<Client>,
    cwd: &PathBuf,
    previous_thread_id: Option<String>,
) -> Result<(String, bool), agent_client_protocol::Error> {
    if let Some(thread_id) = previous_thread_id {
        let resume_id = client
            .send_request(
                "thread/resume",
                json!({
                    "threadId": thread_id,
                    "cwd": cwd,
                }),
            )
            .await?;
        if let Ok(response) =
            wait_for_app_server_response(client, resume_id, session_id, connection).await
            && let Some(resumed_id) = extract_codex_thread_id(&response)
        {
            return Ok((resumed_id, true));
        }
    }

    let start_id = client
        .send_request(
            "thread/start",
            json!({
                "cwd": cwd,
            }),
        )
        .await?;
    let response = wait_for_app_server_response(client, start_id, session_id, connection).await?;
    let thread_id = extract_codex_thread_id(&response).ok_or_else(|| {
        agent_client_protocol::util::internal_error(
            "codex app-server thread/start missing thread id",
        )
    })?;
    Ok((thread_id, false))
}

fn extract_codex_thread_id(response: &Value) -> Option<String> {
    response
        .get("thread")
        .and_then(|thread| thread.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

async fn wait_for_app_server_response(
    client: &mut CodexAppServerClient,
    expected_id: u64,
    session_id: &SessionId,
    connection: &ConnectionTo<Client>,
) -> Result<Value, agent_client_protocol::Error> {
    loop {
        let Some(message) = client.read_json().await? else {
            return Err(agent_client_protocol::util::internal_error(
                "codex app-server exited before responding",
            ));
        };
        if message.get("id").and_then(Value::as_u64) == Some(expected_id) {
            if let Some(error) = message.get("error") {
                let message = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("codex app-server request failed");
                return Err(agent_client_protocol::util::internal_error(message));
            }
            return Ok(message.get("result").cloned().unwrap_or(Value::Null));
        }
        handle_codex_app_server_message(client, session_id, connection, message).await?;
    }
}

async fn read_codex_turn_notifications(
    client: &mut CodexAppServerClient,
    child: &mut Child,
    session_id: &SessionId,
    connection: &ConnectionTo<Client>,
    thread_id: &str,
    codex_turn_id: Option<String>,
    mut cancel_rx: mpsc::UnboundedReceiver<()>,
) -> Result<StopReason, agent_client_protocol::Error> {
    loop {
        tokio::select! {
            _ = cancel_rx.recv() => {
                if let Some(turn_id) = codex_turn_id.as_deref() {
                    send_codex_turn_interrupt(client, thread_id, turn_id).await?;
                    match timeout(
                        Duration::from_secs(2),
                        wait_for_codex_turn_completed(client, session_id, connection),
                    ).await {
                        Ok(Ok(())) => return Ok(StopReason::Cancelled),
                        Ok(Err(error)) => return Err(error),
                        Err(_) => {}
                    }
                }
                let _ = child.kill().await;
                let _ = child.wait().await;
                return Ok(StopReason::Cancelled);
            }
            message = client.read_json() => {
                let Some(message) = message? else {
                    return Err(agent_client_protocol::util::internal_error(
                        "codex app-server exited before turn completed",
                    ));
                };
                if is_codex_turn_completed(&message) {
                    return Ok(StopReason::EndTurn);
                }
                handle_codex_app_server_message(client, session_id, connection, message).await?;
            }
        }
    }
}

async fn wait_for_codex_turn_completed(
    client: &mut CodexAppServerClient,
    session_id: &SessionId,
    connection: &ConnectionTo<Client>,
) -> Result<(), agent_client_protocol::Error> {
    loop {
        let Some(message) = client.read_json().await? else {
            return Ok(());
        };
        if is_codex_turn_completed(&message) {
            return Ok(());
        }
        handle_codex_app_server_message(client, session_id, connection, message).await?;
    }
}

async fn send_codex_turn_interrupt(
    client: &mut CodexAppServerClient,
    thread_id: &str,
    turn_id: &str,
) -> Result<(), agent_client_protocol::Error> {
    let request_id = client
        .send_request(
            "turn/interrupt",
            json!({
                "threadId": thread_id,
                "turnId": turn_id,
            }),
        )
        .await?;
    // The completion notification is authoritative for the ACP stop reason;
    // this response only confirms that Codex accepted the interrupt request.
    let _ = request_id;
    Ok(())
}

fn is_codex_turn_completed(message: &Value) -> bool {
    message.get("method").and_then(Value::as_str) == Some("turn/completed")
}

async fn handle_codex_app_server_message(
    client: &mut CodexAppServerClient,
    session_id: &SessionId,
    connection: &ConnectionTo<Client>,
    message: Value,
) -> Result<(), agent_client_protocol::Error> {
    if message.get("id").is_some() && message.get("method").is_some() {
        respond_to_codex_server_request(client, session_id, connection, &message).await?;
        return Ok(());
    }

    let Some(method) = message.get("method").and_then(Value::as_str) else {
        return Ok(());
    };
    let params = message.get("params").unwrap_or(&Value::Null);
    match method {
        "item/agentMessage/delta" => {
            if let Some(delta) = params.get("delta").and_then(Value::as_str) {
                emit_text_chunk(connection, session_id, delta)?;
            }
        }
        "item/reasoning/summaryTextDelta" | "item/reasoning/textDelta" => {
            if let Some(delta) = params.get("delta").and_then(Value::as_str) {
                emit_thought_chunk(connection, session_id, delta)?;
            }
        }
        "item/started" => emit_codex_item_started(connection, session_id, params)?,
        "item/completed" => emit_codex_item_completed(connection, session_id, params)?,
        "item/commandExecution/outputDelta"
        | "item/fileChange/outputDelta"
        | "item/mcpToolCall/progress" => emit_codex_tool_output(connection, session_id, params)?,
        "warning" | "error" => {
            if let Some(message) = params.get("message").and_then(Value::as_str) {
                emit_thought_chunk(connection, session_id, message)?;
            }
        }
        _ => {}
    }
    Ok(())
}

async fn respond_to_codex_server_request(
    client: &mut CodexAppServerClient,
    session_id: &SessionId,
    connection: &ConnectionTo<Client>,
    message: &Value,
) -> Result<(), agent_client_protocol::Error> {
    let Some(id) = message.get("id").cloned() else {
        return Ok(());
    };
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    // Approval and dynamic-tool callbacks are host authority boundaries. Until
    // OxideTerm has a dedicated Codex permission UI, default to the least
    // privileged response rather than implicitly granting app-server requests.
    match method {
        "item/commandExecution/requestApproval" => {
            let params = message.get("params").unwrap_or(&Value::Null);
            let approved =
                request_codex_approval_via_acp(connection, session_id, params, ToolKind::Execute)
                    .await?;
            let decision = if approved { "accept" } else { "decline" };
            client.send_response(id, json!({"decision": decision})).await
        }
        "item/fileChange/requestApproval" => {
            let params = message.get("params").unwrap_or(&Value::Null);
            let approved =
                request_codex_approval_via_acp(connection, session_id, params, ToolKind::Edit)
                    .await?;
            let decision = if approved { "accept" } else { "decline" };
            client.send_response(id, json!({"decision": decision})).await
        }
        "item/permissions/requestApproval" => {
            client
                .send_response(id, json!({"permissions": {}, "scope": "turn"}))
                .await
        }
        "item/tool/requestUserInput" => client.send_response(id, json!({"answers": {}})).await,
        "mcpServer/elicitation/request" => {
            client.send_response(id, json!({"action": "decline"})).await
        }
        "item/tool/call" => {
            client
                .send_response(
                    id,
                    json!({
                        "success": false,
                        "contentItems": [{
                            "type": "inputText",
                            "text": "OxideTerm Codex app-server bridge does not expose client dynamic tools yet.",
                        }],
                    }),
                )
                .await
        }
        _ => {
            client
                .send_error_response(id, "unsupported Codex app-server request")
                .await
        }
    }
}

async fn request_codex_approval_via_acp(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    params: &Value,
    kind: ToolKind,
) -> Result<bool, agent_client_protocol::Error> {
    let tool_call_id = params
        .get("approvalId")
        .and_then(Value::as_str)
        .or_else(|| params.get("itemId").and_then(Value::as_str))
        .map(str::to_string)
        .unwrap_or_else(|| format!("codex-approval-{}", Uuid::new_v4()));
    let title = codex_approval_title(params, kind);
    let tool_call = ToolCallUpdate::new(
        tool_call_id,
        ToolCallUpdateFields::new()
            .kind(kind)
            .status(ToolCallStatus::Pending)
            .title(Some(title))
            .raw_input(Some(params.clone())),
    );
    let request = RequestPermissionRequest::new(
        session_id.clone(),
        tool_call,
        vec![
            PermissionOption::new("allow_once", "Allow once", PermissionOptionKind::AllowOnce),
            PermissionOption::new("reject_once", "Reject", PermissionOptionKind::RejectOnce),
        ],
    );
    let (tx, rx) = oneshot::channel();
    let request_connection = connection.clone();
    connection.spawn(async move {
        // Permission responses must be awaited from a spawned ACP task; blocking
        // from the prompt handler can deadlock the connection dispatcher.
        let response = request_connection.send_request(request).block_task().await;
        let _ = tx.send(response);
        Ok(())
    })?;
    let response = rx.await.map_err(|_| {
        agent_client_protocol::util::internal_error("ACP permission response channel closed")
    })??;
    Ok(matches!(
        response.outcome,
        RequestPermissionOutcome::Selected(selected)
            if selected.option_id.0.as_ref() == "allow_once"
    ))
}

fn codex_approval_title(params: &Value, kind: ToolKind) -> String {
    match kind {
        ToolKind::Execute => params
            .get("command")
            .and_then(Value::as_str)
            .filter(|command| !command.is_empty())
            .unwrap_or("Command approval")
            .to_string(),
        ToolKind::Edit => params
            .get("reason")
            .and_then(Value::as_str)
            .filter(|reason| !reason.is_empty())
            .unwrap_or("File change approval")
            .to_string(),
        _ => "Approval required".to_string(),
    }
}

fn emit_codex_item_started(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    params: &Value,
) -> Result<(), agent_client_protocol::Error> {
    let Some(item) = params.get("item") else {
        return Ok(());
    };
    let Some(item_id) = item.get("id").and_then(Value::as_str) else {
        return Ok(());
    };
    let item_type = item.get("type").and_then(Value::as_str).unwrap_or("tool");
    let (title, kind) = codex_item_title_and_kind(item_type, item);
    connection.send_notification(SessionNotification::new(
        session_id.clone(),
        SessionUpdate::ToolCall(
            ToolCall::new(item_id.to_string(), title)
                .kind(kind)
                .status(ToolCallStatus::InProgress)
                .raw_input(Some(item.clone())),
        ),
    ))?;
    Ok(())
}

fn emit_codex_item_completed(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    params: &Value,
) -> Result<(), agent_client_protocol::Error> {
    let Some(item) = params.get("item") else {
        return Ok(());
    };
    let Some(item_id) = item.get("id").and_then(Value::as_str) else {
        return Ok(());
    };
    let status = codex_item_completion_status(item);
    connection.send_notification(SessionNotification::new(
        session_id.clone(),
        SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            item_id.to_string(),
            ToolCallUpdateFields::new()
                .status(status)
                .raw_output(Some(item.clone())),
        )),
    ))?;
    Ok(())
}

fn emit_codex_tool_output(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    params: &Value,
) -> Result<(), agent_client_protocol::Error> {
    let Some(item_id) = params.get("itemId").and_then(Value::as_str) else {
        return Ok(());
    };
    let output = params
        .get("delta")
        .or_else(|| params.get("message"))
        .and_then(Value::as_str);
    let Some(output) = output.filter(|output| !output.is_empty()) else {
        return Ok(());
    };
    connection.send_notification(SessionNotification::new(
        session_id.clone(),
        SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            item_id.to_string(),
            ToolCallUpdateFields::new().content(Some(vec![output.to_string().into()])),
        )),
    ))?;
    Ok(())
}

fn codex_item_title_and_kind(item_type: &str, item: &Value) -> (String, ToolKind) {
    match item_type {
        "commandExecution" => (
            item.get("command")
                .and_then(Value::as_str)
                .unwrap_or("Command")
                .to_string(),
            ToolKind::Execute,
        ),
        "fileChange" => ("File change".to_string(), ToolKind::Edit),
        "mcpToolCall" | "dynamicToolCall" | "collabAgentToolCall" => (
            item.get("tool")
                .or_else(|| item.get("toolName"))
                .and_then(Value::as_str)
                .unwrap_or("Tool call")
                .to_string(),
            ToolKind::Other,
        ),
        "webSearch" => ("Web search".to_string(), ToolKind::Search),
        "reasoning" => ("Reasoning".to_string(), ToolKind::Think),
        _ => (item_type.to_string(), ToolKind::Other),
    }
}

fn codex_item_completion_status(item: &Value) -> ToolCallStatus {
    match item.get("status").and_then(Value::as_str) {
        Some("failed" | "error" | "cancelled") => ToolCallStatus::Failed,
        _ => ToolCallStatus::Completed,
    }
}

async fn read_claude_stream_json_stdout(
    config: &AdapterConfig,
    session_id: &SessionId,
    connection: &ConnectionTo<Client>,
    child: &mut Child,
    stdout: ChildStdout,
    mut cancel_rx: mpsc::UnboundedReceiver<()>,
) -> Result<ProviderOutcome, agent_client_protocol::Error> {
    let mut stdout = BufReader::new(stdout);
    let mut line = String::new();
    let mut claude_session_id = None;

    loop {
        line.clear();
        tokio::select! {
            _ = cancel_rx.recv() => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                return Ok(ProviderOutcome {
                    stop_reason: StopReason::Cancelled,
                    claude_session_id,
                    codex_thread_id: None,
                });
            }
            read_result = stdout.read_line(&mut line) => {
                let read_len = read_result.map_err(agent_client_protocol::Error::into_internal_error)?;
                if read_len == 0 {
                    break;
                }
                let value = serde_json::from_str::<Value>(line.trim_end())
                    .map_err(agent_client_protocol::Error::into_internal_error)?;
                if let Some(session_id) = handle_claude_stream_json_message(connection, session_id, &value)? {
                    claude_session_id = Some(session_id);
                }
            }
        }
    }

    let status = child
        .wait()
        .await
        .map_err(agent_client_protocol::Error::into_internal_error)?;
    if status.success() {
        Ok(ProviderOutcome {
            stop_reason: StopReason::EndTurn,
            claude_session_id,
            codex_thread_id: None,
        })
    } else {
        Err(agent_client_protocol::util::internal_error(format!(
            "{} command exited unsuccessfully",
            config.provider.agent_name()
        )))
    }
}

fn handle_claude_stream_json_message(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    value: &Value,
) -> Result<Option<String>, agent_client_protocol::Error> {
    let claude_session_id = value
        .get("session_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    match value.get("type").and_then(Value::as_str) {
        Some("stream_event") => handle_claude_stream_event(connection, session_id, value)?,
        Some("system") => handle_claude_system_event(connection, session_id, value)?,
        Some("error") => {
            if let Some(message) = value.get("message").and_then(Value::as_str) {
                emit_thought_chunk(connection, session_id, message)?;
            }
        }
        _ => {}
    }
    Ok(claude_session_id)
}

fn handle_claude_stream_event(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    value: &Value,
) -> Result<(), agent_client_protocol::Error> {
    let event = value.get("event").unwrap_or(&Value::Null);
    if let Some(delta) = event.get("delta") {
        match delta.get("type").and_then(Value::as_str) {
            Some("text_delta") => {
                if let Some(text) = delta.get("text").and_then(Value::as_str) {
                    emit_text_chunk(connection, session_id, text)?;
                }
            }
            Some("thinking_delta") => {
                if let Some(text) = delta.get("thinking").and_then(Value::as_str) {
                    emit_thought_chunk(connection, session_id, text)?;
                }
            }
            Some("input_json_delta") => {
                if let Some(delta) = delta.get("partial_json").and_then(Value::as_str) {
                    emit_claude_tool_delta(connection, session_id, event, delta)?;
                }
            }
            _ => {}
        }
    }
    match event.get("type").and_then(Value::as_str) {
        Some("content_block_start") => {
            emit_claude_content_block_start(connection, session_id, event)?
        }
        Some("content_block_stop") => {
            emit_claude_content_block_stop(connection, session_id, event)?
        }
        _ => {}
    }
    Ok(())
}

fn handle_claude_system_event(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    value: &Value,
) -> Result<(), agent_client_protocol::Error> {
    let subtype = value
        .get("subtype")
        .and_then(Value::as_str)
        .unwrap_or("system");
    match subtype {
        "api_retry" => {
            let attempt = value.get("attempt").and_then(Value::as_u64).unwrap_or(0);
            let max_retries = value
                .get("max_retries")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            emit_thought_chunk(
                connection,
                session_id,
                &format!("Claude Code API retry {attempt}/{max_retries}"),
            )?;
        }
        "plugin_install" => {
            if let Some(status) = value.get("status").and_then(Value::as_str) {
                emit_thought_chunk(connection, session_id, &format!("Claude plugin {status}"))?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn emit_claude_content_block_start(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    event: &Value,
) -> Result<(), agent_client_protocol::Error> {
    let Some(block) = event.get("content_block") else {
        return Ok(());
    };
    if block.get("type").and_then(Value::as_str) != Some("tool_use") {
        return Ok(());
    }
    let Some(tool_call_id) = block.get("id").and_then(Value::as_str) else {
        return Ok(());
    };
    let name = block
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("Claude tool");
    connection.send_notification(SessionNotification::new(
        session_id.clone(),
        SessionUpdate::ToolCall(
            ToolCall::new(tool_call_id.to_string(), name.to_string())
                .kind(claude_tool_kind(name))
                .status(ToolCallStatus::InProgress)
                .raw_input(Some(block.clone())),
        ),
    ))?;
    Ok(())
}

fn emit_claude_content_block_stop(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    event: &Value,
) -> Result<(), agent_client_protocol::Error> {
    let Some(tool_call_id) = claude_tool_call_id_from_event(event) else {
        return Ok(());
    };
    connection.send_notification(SessionNotification::new(
        session_id.clone(),
        SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            tool_call_id,
            ToolCallUpdateFields::new().status(ToolCallStatus::Completed),
        )),
    ))?;
    Ok(())
}

fn emit_claude_tool_delta(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    event: &Value,
    delta: &str,
) -> Result<(), agent_client_protocol::Error> {
    let Some(tool_call_id) = claude_tool_call_id_from_event(event) else {
        return Ok(());
    };
    connection.send_notification(SessionNotification::new(
        session_id.clone(),
        SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            tool_call_id,
            ToolCallUpdateFields::new().content(Some(vec![delta.to_string().into()])),
        )),
    ))?;
    Ok(())
}

fn claude_tool_call_id_from_event(event: &Value) -> Option<String> {
    event
        .get("content_block")
        .and_then(|block| block.get("id"))
        .or_else(|| event.get("content_block_id"))
        .or_else(|| event.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn claude_tool_kind(name: &str) -> ToolKind {
    match name {
        "Bash" => ToolKind::Execute,
        "Edit" | "MultiEdit" | "Write" | "NotebookEdit" => ToolKind::Edit,
        "Grep" | "Glob" | "WebSearch" => ToolKind::Search,
        "Read" | "LS" => ToolKind::Read,
        _ => ToolKind::Other,
    }
}

fn emit_text_chunk(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    text: &str,
) -> Result<(), agent_client_protocol::Error> {
    if !text.is_empty() {
        connection.send_notification(SessionNotification::new(
            session_id.clone(),
            SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::from(text))),
        ))?;
    }
    Ok(())
}

fn emit_thought_chunk(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    text: &str,
) -> Result<(), agent_client_protocol::Error> {
    if !text.is_empty() {
        connection.send_notification(SessionNotification::new(
            session_id.clone(),
            SessionUpdate::AgentThoughtChunk(ContentChunk::new(ContentBlock::from(text))),
        ))?;
    }
    Ok(())
}

fn prompt_text(blocks: &[ContentBlock]) -> String {
    blocks
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}
