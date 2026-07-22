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
        ConfigOptionUpdate, ContentBlock, ContentChunk, DeleteSessionRequest,
        DeleteSessionResponse, Implementation, InitializeRequest, InitializeResponse,
        NewSessionRequest, NewSessionResponse, PermissionOption, PermissionOptionKind,
        PromptRequest, PromptResponse, ProtocolVersion, RequestPermissionOutcome,
        RequestPermissionRequest, SessionConfigOption, SessionConfigOptionCategory,
        SessionConfigSelectOption, SessionId, SessionNotification, SessionUpdate,
        SetSessionConfigOptionRequest, SetSessionConfigOptionResponse, StopReason, ToolCall,
        ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields, ToolKind,
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

mod claude;
mod codex;

use claude::stream_claude_code_provider;
use codex::{discover_codex_models, stream_codex_app_server_provider};

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
    models: Vec<AdapterModel>,
    selected_model: Option<String>,
    observed_model: Option<String>,
    accepts_unlisted_models: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AdapterModel {
    id: String,
    name: String,
    description: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct CodexModelCatalog {
    models: Vec<AdapterModel>,
    selected_model: Option<String>,
}

const ACP_MODEL_CONFIG_ID: &str = "model";

type Sessions = Arc<Mutex<HashMap<SessionId, SessionState>>>;
type ActiveRuns = Arc<Mutex<HashMap<SessionId, ActiveRun>>>;

struct ProviderOutcome {
    stop_reason: StopReason,
    claude_session_id: Option<String>,
    codex_thread_id: Option<String>,
    resolved_model: Option<String>,
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
                let config = Arc::clone(&config);
                let sessions = Arc::clone(&sessions);
                async move |request: NewSessionRequest, responder, _connection| {
                    let session_id = SessionId::new(format!(
                        "oxideterm-{}-{}",
                        env!("CARGO_PKG_VERSION"),
                        Uuid::new_v4()
                    ));
                    let catalog = if config.provider == AdapterProvider::Codex {
                        // Discovery owns staged timeouts so a slow optional model listing cannot
                        // discard a current model already returned by Codex configuration.
                        discover_codex_models(&config, &request.cwd)
                            .await
                            .unwrap_or_default()
                    } else {
                        CodexModelCatalog::default()
                    };
                    // Keep model choice beside the session so it never becomes global CLI state.
                    let state = SessionState {
                        cwd: request.cwd,
                        claude_session_id: None,
                        codex_thread_id: None,
                        models: catalog.models,
                        selected_model: catalog.selected_model,
                        observed_model: None,
                        accepts_unlisted_models: config.provider == AdapterProvider::ClaudeCode,
                    };
                    let config_options = session_config_options(&state);
                    sessions
                        .lock()
                        .expect("session registry lock")
                        .insert(session_id.clone(), state);
                    responder.respond(
                        NewSessionResponse::new(session_id)
                            .config_options((!config_options.is_empty()).then_some(config_options)),
                    )
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let sessions = Arc::clone(&sessions);
                async move |request: SetSessionConfigOptionRequest, responder, _connection| {
                    match set_session_config_option(&sessions, request) {
                        Ok(response) => responder.respond(response),
                        Err(error) => responder.respond_with_error(error),
                    }
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

fn session_config_options(state: &SessionState) -> Vec<SessionConfigOption> {
    let Some(selected_model) = state
        .selected_model
        .as_deref()
        .or(state.observed_model.as_deref())
        .filter(|selected| state.models.iter().any(|model| model.id == *selected))
    else {
        return Vec::new();
    };
    let choices = state
        .models
        .iter()
        .map(|model| {
            SessionConfigSelectOption::new(model.id.clone(), model.name.clone())
                .description(model.description.clone())
        })
        .collect::<Vec<_>>();
    vec![
        SessionConfigOption::select(
            ACP_MODEL_CONFIG_ID,
            "Model",
            selected_model.to_string(),
            choices,
        )
        .category(SessionConfigOptionCategory::Model),
    ]
}

fn observed_model_config_options(model: &str) -> Vec<SessionConfigOption> {
    vec![
        SessionConfigOption::select(
            ACP_MODEL_CONFIG_ID,
            "Model",
            model.to_string(),
            vec![
                SessionConfigSelectOption::new(model.to_string(), model.to_string())
                    .description("Model reported by the agent for the current session."),
            ],
        )
        .category(SessionConfigOptionCategory::Model),
    ]
}

fn set_session_config_option(
    sessions: &Sessions,
    request: SetSessionConfigOptionRequest,
) -> Result<SetSessionConfigOptionResponse, agent_client_protocol::Error> {
    if request.config_id.to_string() != ACP_MODEL_CONFIG_ID {
        return Err(agent_client_protocol::util::internal_error(
            "ACP session config option was not found",
        ));
    }
    let mut sessions = sessions.lock().expect("session registry lock");
    let session = sessions
        .get_mut(&request.session_id)
        .ok_or_else(|| agent_client_protocol::util::internal_error("ACP session was not found"))?;
    let value_id = request.value.to_string();
    if !session.models.iter().any(|model| model.id == value_id) {
        if !session.accepts_unlisted_models {
            return Err(agent_client_protocol::util::internal_error(
                "ACP session config value was not found",
            ));
        }
        // Claude Code cannot enumerate models, but it can pin a model observed from system/init.
        session.models.push(AdapterModel {
            id: value_id.clone(),
            name: value_id.clone(),
            description: Some("Model reported by Claude Code.".to_string()),
        });
    }
    session.selected_model = Some(value_id);
    Ok(SetSessionConfigOptionResponse::new(session_config_options(
        session,
    )))
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
    if let Some(resolved_model) = outcome.resolved_model.clone()
        && let Some(session) = sessions
            .lock()
            .expect("session registry lock")
            .get_mut(&request.session_id)
    {
        if !session
            .models
            .iter()
            .any(|model| model.id == resolved_model)
        {
            session.models.push(AdapterModel {
                id: resolved_model.clone(),
                name: resolved_model.clone(),
                description: Some("Model reported by Claude Code.".to_string()),
            });
        }
        session.observed_model = Some(resolved_model);
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
                session.selected_model,
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
                session.selected_model,
                prompt,
                connection,
            )
            .await
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_config_selection_is_session_scoped_and_validated() {
        let session_id = SessionId::new("session-1");
        let sessions = Sessions::default();
        sessions.lock().expect("session registry lock").insert(
            session_id.clone(),
            SessionState {
                cwd: PathBuf::from("/workspace"),
                claude_session_id: None,
                codex_thread_id: None,
                models: vec![
                    AdapterModel {
                        id: "model-a".to_string(),
                        name: "Model A".to_string(),
                        description: None,
                    },
                    AdapterModel {
                        id: "model-b".to_string(),
                        name: "Model B".to_string(),
                        description: None,
                    },
                ],
                selected_model: Some("model-a".to_string()),
                observed_model: None,
                accepts_unlisted_models: false,
            },
        );

        let response = set_session_config_option(
            &sessions,
            SetSessionConfigOptionRequest::new(session_id.clone(), ACP_MODEL_CONFIG_ID, "model-b"),
        )
        .expect("valid model selection");

        assert_eq!(response.config_options.len(), 1);
        assert_eq!(
            sessions
                .lock()
                .expect("session registry lock")
                .get(&session_id)
                .and_then(|state| state.selected_model.as_deref()),
            Some("model-b")
        );
        assert!(
            set_session_config_option(
                &sessions,
                SetSessionConfigOptionRequest::new(
                    session_id,
                    ACP_MODEL_CONFIG_ID,
                    "missing-model",
                ),
            )
            .is_err()
        );
    }

    #[test]
    fn claude_session_can_pin_a_model_previously_reported_at_runtime() {
        let session_id = SessionId::new("claude-session");
        let sessions = Sessions::default();
        sessions.lock().expect("session registry lock").insert(
            session_id.clone(),
            SessionState {
                cwd: PathBuf::from("/workspace"),
                claude_session_id: None,
                codex_thread_id: None,
                models: Vec::new(),
                selected_model: None,
                observed_model: None,
                accepts_unlisted_models: true,
            },
        );

        let response = set_session_config_option(
            &sessions,
            SetSessionConfigOptionRequest::new(
                session_id.clone(),
                ACP_MODEL_CONFIG_ID,
                "claude-sonnet-4-6",
            ),
        )
        .expect("observed Claude model selection");

        assert_eq!(response.config_options.len(), 1);
        assert_eq!(
            sessions
                .lock()
                .expect("session registry lock")
                .get(&session_id)
                .and_then(|state| state.selected_model.as_deref()),
            Some("claude-sonnet-4-6")
        );
    }
}
