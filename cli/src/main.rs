// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! OxideTerm CLI — `oxt` command-line companion.
//!
//! Communicates with the running OxideTerm GUI via IPC
//! (Unix Domain Socket on macOS/Linux, Named Pipe on Windows).

mod connect;
mod escape;
mod output;
mod protocol;
mod ssh_launch;
mod terminal;
mod wire;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;

const CLI_API_CURRENT_VERSION: u64 = 3;
const CLI_API_MIN_SUPPORTED_VERSION: u64 = 2;
const DEFAULT_WATCH_INTERVAL_MS: u64 = 2000;
const DEFAULT_WAIT_INTERVAL_MS: u64 = 500;
const DEFAULT_WAIT_TIMEOUT_MS: u64 = 30000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
enum ExitCode {
    Success = 0,
    Runtime = 1,
    Usage = 2,
    Timeout = 3,
    NotFound = 4,
    Compatibility = 5,
}

impl ExitCode {
    fn as_i32(self) -> i32 {
        self as i32
    }
}

#[derive(Debug)]
struct CliError {
    code: &'static str,
    message: String,
    exit_code: ExitCode,
}

impl CliError {
    fn runtime(message: impl Into<String>) -> Self {
        Self {
            code: "runtime_error",
            message: message.into(),
            exit_code: ExitCode::Runtime,
        }
    }

    fn usage(message: impl Into<String>) -> Self {
        Self {
            code: "usage_error",
            message: message.into(),
            exit_code: ExitCode::Usage,
        }
    }

    fn timeout(message: impl Into<String>) -> Self {
        Self {
            code: "timeout",
            message: message.into(),
            exit_code: ExitCode::Timeout,
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            code: "not_found",
            message: message.into(),
            exit_code: ExitCode::NotFound,
        }
    }

    fn compatibility(message: impl Into<String>) -> Self {
        Self {
            code: "compatibility_error",
            message: message.into(),
            exit_code: ExitCode::Compatibility,
        }
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "error": {
                "code": self.code,
                "message": self.message,
                "exit_code": self.exit_code.as_i32(),
            }
        })
    }
}

impl From<String> for CliError {
    fn from(message: String) -> Self {
        classify_error_message(message)
    }
}

impl From<&str> for CliError {
    fn from(message: &str) -> Self {
        classify_error_message(message.to_string())
    }
}

struct CompatibilityReport {
    status: serde_json::Value,
    warning: Option<String>,
    error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
enum DoctorStatus {
    Ok,
    Warn,
    Fail,
}

impl DoctorStatus {
    fn label(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Warn => "warn",
            Self::Fail => "fail",
        }
    }
}

#[derive(Debug, serde::Serialize)]
struct DoctorCheck {
    id: &'static str,
    title: &'static str,
    status: DoctorStatus,
    summary: String,
    detail: Option<String>,
}

#[derive(Debug, Default, serde::Serialize)]
struct DoctorEndpoint {
    value: Option<String>,
    source: Option<String>,
}

#[derive(Debug, serde::Serialize)]
struct DoctorReport {
    ok: bool,
    cli_version: &'static str,
    binary_path: Option<String>,
    endpoint: DoctorEndpoint,
    warnings: usize,
    failures: usize,
    items: Vec<DoctorCheck>,
}

impl DoctorReport {
    fn new() -> Self {
        Self {
            ok: true,
            cli_version: env!("CARGO_PKG_VERSION"),
            binary_path: None,
            endpoint: DoctorEndpoint::default(),
            warnings: 0,
            failures: 0,
            items: Vec::new(),
        }
    }

    fn push(&mut self, check: DoctorCheck) {
        match check.status {
            DoctorStatus::Warn => self.warnings += 1,
            DoctorStatus::Fail => self.failures += 1,
            DoctorStatus::Ok => {}
        }
        self.items.push(check);
        self.ok = self.failures == 0;
    }
}

#[derive(Debug)]
struct PathCandidate {
    path: std::path::PathBuf,
    executable: bool,
    broken_symlink: bool,
}

#[derive(Parser)]
#[command(
    name = "oxt",
    about = "OxideTerm CLI companion — control OxideTerm from the command line",
    version
)]
struct Cli {
    /// Force JSON output (default: auto-detect based on terminal/pipe)
    #[arg(long, global = true)]
    json: bool,

    /// Suppress human-oriented notices and guidance on stderr
    #[arg(long, global = true)]
    quiet: bool,

    /// IPC timeout in milliseconds
    #[arg(long, global = true, default_value = "30000")]
    timeout: u64,

    /// Custom socket path (debugging)
    #[arg(long, global = true)]
    socket: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Show OxideTerm status
    Status {
        /// Keep polling and printing status updates until interrupted
        #[arg(long)]
        watch: bool,

        /// Polling interval in milliseconds when --watch is enabled
        #[arg(long, default_value_t = DEFAULT_WATCH_INTERVAL_MS, value_parser = clap::value_parser!(u64).range(1..))]
        interval: u64,
    },

    /// List resources
    List {
        #[command(subcommand)]
        what: ListTarget,
    },

    /// Inspect active SSH sessions
    Session {
        #[command(subcommand)]
        action: SessionAction,
    },

    /// Show connection health
    Health {
        /// Session ID (omit to show all)
        session_id: Option<String>,

        /// Keep polling and printing health updates until interrupted
        #[arg(long)]
        watch: bool,

        /// Polling interval in milliseconds when --watch is enabled
        #[arg(long, default_value_t = DEFAULT_WATCH_INTERVAL_MS, value_parser = clap::value_parser!(u64).range(1..))]
        interval: u64,
    },

    /// Disconnect a session
    Disconnect {
        /// Session ID or name to disconnect
        target: String,
    },

    /// Manage port forwarding
    Forward {
        #[command(subcommand)]
        action: ForwardAction,
    },

    /// Query saved configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Ask AI a question (pipe context via stdin)
    Ask {
        /// Prompt text (remaining args joined)
        #[arg(trailing_var_arg = true)]
        prompt: Vec<String>,

        /// Attach terminal buffer from a session as context
        #[arg(short, long)]
        session: Option<String>,

        /// AI model to use (overrides default)
        #[arg(short, long)]
        model: Option<String>,

        /// Provider type: openai, anthropic, gemini, ollama
        #[arg(short, long)]
        provider: Option<String>,

        /// Disable streaming (wait for full response)
        #[arg(long)]
        no_stream: bool,

        /// Force raw text output (no markdown rendering)
        #[arg(long)]
        raw: bool,

        /// Continue a previous conversation by ID
        #[arg(long, short = 'c')]
        r#continue: Option<String>,
    },

    /// Generate code/commands with AI (code-only output)
    Exec {
        /// Prompt text (remaining args joined)
        #[arg(trailing_var_arg = true)]
        prompt: Vec<String>,

        /// Attach terminal buffer from a session as context
        #[arg(short, long)]
        session: Option<String>,

        /// AI model to use
        #[arg(short, long)]
        model: Option<String>,

        /// Provider type
        #[arg(short, long)]
        provider: Option<String>,

        /// Target shell for generated commands
        #[arg(long, value_enum)]
        shell: Option<ExecShell>,

        /// Output format for generated execution plans
        #[arg(long, value_enum, default_value_t = ExecFormat::Text)]
        format: ExecFormat,

        /// Disable streaming and wait for the full response
        #[arg(long)]
        no_stream: bool,
    },

    /// Connect to a saved connection (opens in GUI)
    Connect {
        /// Connection name, host, or ID
        target: String,

        /// Wait until the resulting session appears in the session list
        #[arg(long)]
        wait: bool,

        /// Focus the resulting tab after the session appears
        #[arg(long)]
        focus: bool,

        /// Polling interval in milliseconds when --wait is enabled
        #[arg(long, default_value_t = DEFAULT_WAIT_INTERVAL_MS, value_parser = clap::value_parser!(u64).range(1..))]
        interval: u64,

        /// Maximum time to wait for the session to appear, in milliseconds
        #[arg(long = "wait-timeout", default_value_t = DEFAULT_WAIT_TIMEOUT_MS, value_parser = clap::value_parser!(u64).range(1..))]
        wait_timeout: u64,
    },

    /// Open a temporary SSH connection in the GUI
    Ssh {
        /// SSH target in user@host form
        target: String,

        /// SSH port
        #[arg(short = 'p', long, default_value_t = 22, value_parser = clap::value_parser!(u16).range(1..))]
        port: u16,

        /// Read the SSH password from stdin
        #[arg(long)]
        password_stdin: bool,
    },

    /// Open a new local terminal tab
    Open {
        /// Working directory (default: current directory)
        path: Option<String>,
    },

    /// Focus an existing session tab
    Focus {
        /// Focus the most recently created active tab
        #[arg(long, conflicts_with = "target")]
        latest: bool,

        /// Session ID or name (omit to list available sessions)
        target: Option<String>,
    },

    /// Attach to a running session (mirror terminal I/O)
    Attach {
        /// Attach to the most recently created active session
        #[arg(long, conflicts_with = "target")]
        latest: bool,

        /// Mirror output only; suppress stdin forwarding to the session
        #[arg(long)]
        readonly: bool,

        /// Session ID or name (omit to list available sessions)
        target: Option<String>,
    },

    /// SFTP file operations
    Sftp {
        #[command(subcommand)]
        action: SftpAction,
    },

    /// Import connections from ~/.ssh/config
    Import {
        #[command(subcommand)]
        action: ImportAction,
    },

    /// Ping the GUI (connectivity check)
    Ping,

    /// Diagnose CLI installation and GUI connectivity
    Doctor,

    /// Show version information
    Version,

    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        shell: Shell,
    },
}

#[derive(Subcommand)]
enum ListTarget {
    /// List saved connections
    Connections,
    /// List active sessions
    Sessions,
    /// List active SSH connections in the pool
    ActiveConnections,
    /// List active local terminal tabs
    LocalTerminals,
    /// List active port forwards
    Forwards {
        /// Session ID (omit to show all sessions)
        session_id: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
enum ExecShell {
    Bash,
    Zsh,
    Fish,
    #[value(name = "powershell", alias = "power-shell")]
    PowerShell,
}

impl ExecShell {
    fn as_str(self) -> &'static str {
        match self {
            Self::Bash => "bash",
            Self::Zsh => "zsh",
            Self::Fish => "fish",
            Self::PowerShell => "powershell",
        }
    }

    fn parse_flag_value(value: &str) -> Option<Self> {
        match value {
            "bash" => Some(Self::Bash),
            "zsh" => Some(Self::Zsh),
            "fish" => Some(Self::Fish),
            "powershell" | "power-shell" => Some(Self::PowerShell),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
enum ExecFormat {
    Text,
    Json,
}

impl ExecFormat {
    fn parse_flag_value(value: &str) -> Option<Self> {
        match value {
            "text" => Some(Self::Text),
            "json" => Some(Self::Json),
            _ => None,
        }
    }
}

#[derive(Debug, Default)]
struct AskPromptTailOptions {
    prompt_tokens: Vec<String>,
    session: Option<String>,
    model: Option<String>,
    provider: Option<String>,
    conversation_id: Option<String>,
    no_stream: bool,
    raw: bool,
}

#[derive(Debug, Default)]
struct ExecPromptTailOptions {
    prompt_tokens: Vec<String>,
    session: Option<String>,
    model: Option<String>,
    provider: Option<String>,
    shell: Option<ExecShell>,
    format: Option<ExecFormat>,
    no_stream: bool,
}

fn default_exec_shell() -> ExecShell {
    #[cfg(windows)]
    {
        return ExecShell::PowerShell;
    }

    #[cfg(not(windows))]
    {
        if let Ok(shell) = std::env::var("SHELL") {
            if shell.ends_with("/zsh") {
                return ExecShell::Zsh;
            }
            if shell.ends_with("/fish") {
                return ExecShell::Fish;
            }
        }

        ExecShell::Bash
    }
}

fn split_ask_prompt_and_tail_args(prompt: &[String]) -> AskPromptTailOptions {
    let mut remaining = prompt.to_vec();
    let mut overrides = AskPromptTailOptions::default();

    loop {
        let len = remaining.len();
        if len == 0 {
            break;
        }

        match remaining[len - 1].as_str() {
            "--no-stream" => {
                overrides.no_stream = true;
                remaining.pop();
                continue;
            }
            "--raw" => {
                overrides.raw = true;
                remaining.pop();
                continue;
            }
            _ => {}
        }

        if len >= 2 {
            let value = remaining[len - 1].clone();
            match remaining[len - 2].as_str() {
                "-s" | "--session" => {
                    overrides.session = Some(value);
                    remaining.truncate(len - 2);
                    continue;
                }
                "-m" | "--model" => {
                    overrides.model = Some(value);
                    remaining.truncate(len - 2);
                    continue;
                }
                "-p" | "--provider" => {
                    overrides.provider = Some(value);
                    remaining.truncate(len - 2);
                    continue;
                }
                "-c" | "--continue" => {
                    overrides.conversation_id = Some(value);
                    remaining.truncate(len - 2);
                    continue;
                }
                _ => {}
            }
        }

        break;
    }

    overrides.prompt_tokens = remaining;
    overrides
}

fn split_exec_prompt_and_tail_args(prompt: &[String]) -> Result<ExecPromptTailOptions, CliError> {
    let mut remaining = prompt.to_vec();
    let mut overrides = ExecPromptTailOptions::default();

    loop {
        let len = remaining.len();
        if len == 0 {
            break;
        }

        if remaining[len - 1] == "--no-stream" {
            overrides.no_stream = true;
            remaining.pop();
            continue;
        }

        if len >= 2 {
            let value = remaining[len - 1].clone();
            match remaining[len - 2].as_str() {
                "-s" | "--session" => {
                    overrides.session = Some(value);
                    remaining.truncate(len - 2);
                    continue;
                }
                "-m" | "--model" => {
                    overrides.model = Some(value);
                    remaining.truncate(len - 2);
                    continue;
                }
                "-p" | "--provider" => {
                    overrides.provider = Some(value);
                    remaining.truncate(len - 2);
                    continue;
                }
                "--shell" => {
                    overrides.shell = Some(ExecShell::parse_flag_value(&value).ok_or_else(|| {
                        CliError::usage(format!(
                            "Invalid shell target: {value}. Expected one of bash, zsh, fish, powershell"
                        ))
                    })?);
                    remaining.truncate(len - 2);
                    continue;
                }
                "--format" => {
                    overrides.format =
                        Some(ExecFormat::parse_flag_value(&value).ok_or_else(|| {
                            CliError::usage(format!(
                                "Invalid exec output format: {value}. Expected text or json"
                            ))
                        })?);
                    remaining.truncate(len - 2);
                    continue;
                }
                _ => {}
            }
        }

        break;
    }

    overrides.prompt_tokens = remaining;
    Ok(overrides)
}

#[derive(Subcommand)]
enum SessionAction {
    /// Inspect an active SSH session
    Inspect {
        /// Session ID, exact name, or unique ID prefix
        target: String,
    },
}

#[derive(Subcommand)]
enum ForwardAction {
    /// Add a port forward to a session
    Add {
        /// Forward spec: [bind_addr:]bind_port:target_host:target_port
        spec: String,

        /// Session ID or name
        #[arg(short, long)]
        session: String,

        /// Forward type: local, remote, dynamic
        #[arg(short = 't', long, default_value = "local")]
        r#type: String,

        /// Optional description
        #[arg(short, long)]
        description: Option<String>,
    },

    /// Remove a port forward
    Remove {
        /// Forward ID
        forward_id: String,

        /// Session ID or name
        #[arg(short, long)]
        session: String,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// List connection groups
    List,

    /// Show connection details
    Get {
        /// Connection name or ID
        name: String,
    },
}

#[derive(Subcommand)]
enum SftpAction {
    /// List remote directory contents
    Ls {
        /// Session ID or name
        #[arg(short, long)]
        session: String,

        /// Remote path (default: home directory)
        path: Option<String>,
    },

    /// Download a file from the remote host
    Get {
        /// Session ID or name
        #[arg(short, long)]
        session: String,

        /// Remote file path
        remote: String,

        /// Local destination path (default: current directory)
        local: Option<String>,
    },

    /// Upload a file to the remote host
    Put {
        /// Session ID or name
        #[arg(short, long)]
        session: String,

        /// Local file path
        local: String,

        /// Remote destination path
        remote: String,
    },
}

#[derive(Subcommand)]
enum ImportAction {
    /// List importable hosts from ~/.ssh/config
    List,

    /// Import specific hosts (or all with --all)
    Add {
        /// Host aliases to import
        aliases: Vec<String>,

        /// Import all available hosts
        #[arg(long)]
        all: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    let out = output::OutputMode::detect(cli.json);

    let result = run(&cli, &out);

    match result {
        Ok(code) => {
            if code != ExitCode::Success {
                std::process::exit(code.as_i32());
            }
        }
        Err(error) => {
            if out.is_json() {
                println!(
                    "{}",
                    serde_json::to_string(&error.to_json()).unwrap_or_default()
                );
            } else {
                eprintln!("error: {}", error.message);
            }
            std::process::exit(error.exit_code.as_i32());
        }
    }
}

fn run(cli: &Cli, out: &output::OutputMode) -> Result<ExitCode, CliError> {
    // Commands that don't need IPC
    match &cli.command {
        Commands::Version => {
            out.print_version();
            return Ok(ExitCode::Success);
        }
        Commands::Completions { shell } => {
            clap_complete::generate(*shell, &mut Cli::command(), "oxt", &mut std::io::stdout());
            return Ok(ExitCode::Success);
        }
        Commands::Doctor => {
            let report = build_doctor_report(cli);
            out.print_doctor(&report);
            return Ok(if report.ok {
                ExitCode::Success
            } else {
                ExitCode::Runtime
            });
        }
        _ => {}
    }

    let mut conn = connect::IpcConnection::connect(cli.socket.as_deref(), cli.timeout)?;
    let compatibility = fetch_compatibility_report(&mut conn)?;

    match &cli.command {
        Commands::Status { watch, interval } => {
            if *watch {
                watch_status(&mut conn, out, cli.quiet, compatibility, *interval)?;
            } else {
                out.print_status(&compatibility.status);
                emit_compatibility_notice(
                    out,
                    cli.quiet,
                    compatibility
                        .error
                        .as_deref()
                        .or(compatibility.warning.as_deref()),
                );
            }
        }
        Commands::List { what } => match what {
            _ if compatibility.error.is_some() => {
                return Err(CliError::compatibility(
                    compatibility.error.unwrap_or_default(),
                ));
            }
            ListTarget::Connections => {
                emit_compatibility_notice(out, cli.quiet, compatibility.warning.as_deref());
                let resp = conn.call("list_saved_connections", serde_json::json!({}))?;
                out.print_connections(&resp);
            }
            ListTarget::Sessions => {
                emit_compatibility_notice(out, cli.quiet, compatibility.warning.as_deref());
                let resp = conn.call("list_sessions", serde_json::json!({}))?;
                out.print_sessions(&resp);
            }
            ListTarget::ActiveConnections => {
                emit_compatibility_notice(out, cli.quiet, compatibility.warning.as_deref());
                let resp = conn.call("list_active_connections", serde_json::json!({}))?;
                out.print_active_connections(&resp);
            }
            ListTarget::LocalTerminals => {
                emit_compatibility_notice(out, cli.quiet, compatibility.warning.as_deref());
                let resp = conn.call("list_local_terminals", serde_json::json!({}))?;
                out.print_local_terminals(&resp);
            }
            ListTarget::Forwards { session_id } => {
                emit_compatibility_notice(out, cli.quiet, compatibility.warning.as_deref());
                let params = match session_id {
                    Some(id) => serde_json::json!({ "session_id": id }),
                    None => serde_json::json!({}),
                };
                let resp = conn.call("list_forwards", params)?;
                out.print_forwards(&resp);
            }
        },
        Commands::Session { action } => match action {
            SessionAction::Inspect { target } => {
                return_if_incompatible(&compatibility)?;
                emit_compatibility_notice(out, cli.quiet, compatibility.warning.as_deref());
                let resp = build_session_inspect(&mut conn, target)?;
                out.print_session_inspect(&resp);
            }
        },
        Commands::Health {
            session_id,
            watch,
            interval,
        } => {
            return_if_incompatible(&compatibility)?;
            emit_compatibility_notice(out, cli.quiet, compatibility.warning.as_deref());
            if *watch {
                watch_health(&mut conn, out, session_id.as_deref(), *interval)?;
            } else {
                let params = match session_id {
                    Some(id) => serde_json::json!({ "session_id": id }),
                    None => serde_json::json!({}),
                };
                let resp = conn.call("health", params)?;
                out.print_health(&resp, session_id.is_some());
            }
        }
        Commands::Disconnect { target } => {
            return_if_incompatible(&compatibility)?;
            emit_compatibility_notice(out, cli.quiet, compatibility.warning.as_deref());
            let resp = conn.call("disconnect", serde_json::json!({ "target": target }))?;
            out.print_disconnect(&resp);
        }
        Commands::Forward { action } => match action {
            ForwardAction::Add {
                spec,
                session,
                r#type,
                description,
            } => {
                return_if_incompatible(&compatibility)?;
                emit_compatibility_notice(out, cli.quiet, compatibility.warning.as_deref());
                let (bind_address, bind_port, target_host, target_port) =
                    parse_forward_spec(spec, r#type)?;

                // Resolve session name → ID if needed
                let session_id = resolve_session_id(&mut conn, session)?;

                let mut params = serde_json::json!({
                    "session_id": session_id,
                    "forward_type": r#type,
                    "bind_address": bind_address,
                    "bind_port": bind_port,
                    "target_host": target_host,
                    "target_port": target_port,
                });
                if let Some(desc) = description {
                    params["description"] = serde_json::json!(desc);
                }
                let resp = conn.call("create_forward", params)?;
                out.print_forward_result(&resp);
            }
            ForwardAction::Remove {
                forward_id,
                session,
            } => {
                return_if_incompatible(&compatibility)?;
                emit_compatibility_notice(out, cli.quiet, compatibility.warning.as_deref());
                let session_id = resolve_session_id(&mut conn, session)?;
                let resp = conn.call(
                    "delete_forward",
                    serde_json::json!({
                        "session_id": session_id,
                        "forward_id": forward_id,
                    }),
                )?;
                out.print_forward_result(&resp);
            }
        },
        Commands::Config { action } => match action {
            ConfigAction::List => {
                return_if_incompatible(&compatibility)?;
                emit_compatibility_notice(out, cli.quiet, compatibility.warning.as_deref());
                let resp = conn.call("config_list", serde_json::json!({}))?;
                out.print_config_list(&resp);
            }
            ConfigAction::Get { name } => {
                return_if_incompatible(&compatibility)?;
                emit_compatibility_notice(out, cli.quiet, compatibility.warning.as_deref());
                let resp = conn.call("config_get", serde_json::json!({ "name": name }))?;
                out.print_config_get(&resp);
            }
        },
        Commands::Ask {
            prompt,
            session,
            model,
            provider,
            no_stream,
            raw,
            r#continue: continue_id,
        } => {
            return_if_incompatible(&compatibility)?;
            emit_compatibility_notice(out, cli.quiet, compatibility.warning.as_deref());
            let prompt_overrides = split_ask_prompt_and_tail_args(prompt);
            let prompt_text = prompt_overrides.prompt_tokens.join(" ");
            if prompt_text.is_empty() && is_terminal_stdin() {
                return Err(CliError::usage(
                    "No prompt provided. Usage: oxt ask \"your question\"",
                ));
            }

            // Read stdin if piped
            let context = if !is_terminal_stdin() {
                Some(read_stdin()?)
            } else {
                None
            };

            let mut params = serde_json::json!({
                "prompt": if prompt_text.is_empty() { "Analyze the following".to_string() } else { prompt_text },
                "stream": !(*no_stream || prompt_overrides.no_stream),
            });
            if let Some(ctx) = &context {
                params["context"] = serde_json::json!(ctx);
            }
            if let Some(s) = prompt_overrides.session.as_deref().or(session.as_deref()) {
                let sid = resolve_session_id(&mut conn, s)?;
                params["session_id"] = serde_json::json!(sid);
            }
            if let Some(m) = prompt_overrides.model.as_deref().or(model.as_deref()) {
                params["model"] = serde_json::json!(m);
            }
            if let Some(p) = prompt_overrides.provider.as_deref().or(provider.as_deref()) {
                params["provider"] = serde_json::json!(p);
            }
            if let Some(cid) = prompt_overrides
                .conversation_id
                .as_deref()
                .or(continue_id.as_deref())
            {
                params["conversation_id"] = serde_json::json!(cid);
            }

            // Determine markdown rendering mode:
            // --raw or piped stdout → raw text; TTY stdout → markdown render
            let use_markdown = should_render_ai_markdown(*raw || prompt_overrides.raw, out);
            let emit_json_envelope = cli.json;

            if *no_stream || prompt_overrides.no_stream {
                let resp = conn.call("ask", params)?;
                let text = resp
                    .get("text")
                    .and_then(|v| v.as_str())
                    .or_else(|| resp.get("content").and_then(|v| v.as_str()))
                    .unwrap_or("");
                if use_markdown {
                    render_markdown(text);
                } else if emit_json_envelope {
                    out.print_ai_response(&resp);
                } else {
                    println!("{text}");
                }
                print_conversation_id(&resp, cli.quiet);
            } else {
                let mut accumulated = String::new();
                let mut printed_chunks = false;
                let resp = conn.call_streaming("ask", params, |text| {
                    if use_markdown || emit_json_envelope {
                        accumulated.push_str(text);
                    } else {
                        use std::io::Write;
                        print!("{text}");
                        let _ = std::io::stdout().flush();
                        printed_chunks = true;
                    }
                })?;
                if emit_json_envelope {
                    let mut final_resp = resp.clone();
                    if final_resp.get("text").and_then(|v| v.as_str()).is_none()
                        && !accumulated.is_empty()
                    {
                        final_resp["text"] = serde_json::json!(accumulated);
                    }
                    out.print_ai_response(&final_resp);
                } else if use_markdown {
                    let final_text = if accumulated.is_empty() {
                        resp.get("text")
                            .and_then(|v| v.as_str())
                            .or_else(|| resp.get("content").and_then(|v| v.as_str()))
                            .unwrap_or("")
                    } else {
                        &accumulated
                    };
                    render_markdown(final_text);
                } else if !printed_chunks {
                    let final_text = resp
                        .get("text")
                        .and_then(|v| v.as_str())
                        .or_else(|| resp.get("content").and_then(|v| v.as_str()))
                        .unwrap_or("");
                    println!("{final_text}");
                } else {
                    println!();
                }
                print_conversation_id(&resp, cli.quiet);
            }
        }
        Commands::Exec {
            prompt,
            session,
            model,
            provider,
            shell,
            format,
            no_stream,
        } => {
            return_if_incompatible(&compatibility)?;
            emit_compatibility_notice(out, cli.quiet, compatibility.warning.as_deref());
            let prompt_overrides = split_exec_prompt_and_tail_args(prompt)?;
            let prompt_text = prompt_overrides.prompt_tokens.join(" ");
            if prompt_text.is_empty() && is_terminal_stdin() {
                return Err(CliError::usage(
                    "No prompt provided. Usage: oxt exec \"generate a script\"",
                ));
            }

            let context = if !is_terminal_stdin() {
                Some(read_stdin()?)
            } else {
                None
            };

            let shell_target = prompt_overrides
                .shell
                .or(*shell)
                .unwrap_or_else(default_exec_shell);
            let format = prompt_overrides.format.unwrap_or(*format);
            let streaming = !(*no_stream || prompt_overrides.no_stream);

            let mut params = serde_json::json!({
                "prompt": if prompt_text.is_empty() { "Generate code for the following".to_string() } else { prompt_text },
                "stream": streaming,
                "exec_mode": true,
                "shell_target": shell_target.as_str(),
            });
            if let Some(ctx) = &context {
                params["context"] = serde_json::json!(ctx);
            }
            if let Some(s) = prompt_overrides.session.as_deref().or(session.as_deref()) {
                let sid = resolve_session_id(&mut conn, s)?;
                params["session_id"] = serde_json::json!(sid);
            }
            if let Some(m) = prompt_overrides.model.as_deref().or(model.as_deref()) {
                params["model"] = serde_json::json!(m);
            }
            if let Some(p) = prompt_overrides.provider.as_deref().or(provider.as_deref()) {
                params["provider"] = serde_json::json!(p);
            }

            let resp = if streaming {
                let mut accumulated = String::new();
                let mut resp = conn.call_streaming("ask", params, |text| {
                    accumulated.push_str(text);
                })?;
                if resp.get("text").and_then(|v| v.as_str()).is_none() && !accumulated.is_empty() {
                    resp["text"] = serde_json::json!(accumulated);
                }
                resp
            } else {
                conn.call("ask", params)?
            };

            let exec_output_mode = if cli.json || matches!(format, ExecFormat::Json) {
                output::OutputMode::Json
            } else {
                output::OutputMode::Human
            };

            exec_output_mode.print_exec_result(&resp);

            print_conversation_id(&resp, cli.quiet);
        }
        Commands::Connect {
            target,
            wait,
            focus,
            interval,
            wait_timeout,
        } => {
            return_if_incompatible(&compatibility)?;
            emit_compatibility_notice(out, cli.quiet, compatibility.warning.as_deref());
            let resp = conn.call("connect", serde_json::json!({ "target": target }))?;
            let should_wait = *wait || *focus;
            let mut final_resp = if should_wait {
                wait_for_connected_session(&mut conn, &resp, *interval, *wait_timeout)?
            } else {
                resp
            };
            if *focus {
                let session_id = final_resp
                    .get("session_id")
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| {
                        CliError::runtime(
                            "connect --focus requires the session to appear before focusing",
                        )
                    })?;
                let focus_resp =
                    conn.call("focus_tab", serde_json::json!({ "target": session_id }))?;
                final_resp["focused"] = serde_json::json!(true);
                final_resp["focus_result"] = focus_resp;
            }
            out.print_connect_result(&final_resp);
        }
        Commands::Ssh {
            target,
            port,
            password_stdin,
        } => {
            return_if_incompatible(&compatibility)?;
            emit_compatibility_notice(out, cli.quiet, compatibility.warning.as_deref());
            let password = if *password_stdin {
                Some(ssh_launch::read_password_from_stdin()?)
            } else {
                None
            };
            let launch = ssh_launch::build_launch(target, *port, password.as_ref())?;
            let resp = conn.call(
                "ssh",
                serde_json::to_value(&launch).map_err(|error| {
                    CliError::runtime(format!("Failed to serialize SSH launch request: {error}"))
                })?,
            )?;
            if should_emit_human_guidance(out, cli.quiet) {
                eprintln!("Opening temporary SSH terminal: {}", launch.title());
            }
            out.print_json(&resp);
        }
        Commands::Sftp { action } => match action {
            SftpAction::Ls { session, path } => {
                return_if_incompatible(&compatibility)?;
                emit_compatibility_notice(out, cli.quiet, compatibility.warning.as_deref());
                let session_id = resolve_session_id(&mut conn, session)?;
                let params = serde_json::json!({
                    "session_id": session_id,
                    "path": path.as_deref().unwrap_or("."),
                });
                let resp = conn.call("sftp_ls", params)?;
                out.print_sftp_ls(&resp);
            }
            SftpAction::Get {
                session,
                remote,
                local,
            } => {
                return_if_incompatible(&compatibility)?;
                emit_compatibility_notice(out, cli.quiet, compatibility.warning.as_deref());
                let session_id = resolve_session_id(&mut conn, session)?;
                // Default local path: filename in current directory
                let local_path = local.clone().unwrap_or_else(|| {
                    std::path::Path::new(remote)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "download".to_string())
                });
                let params = serde_json::json!({
                    "session_id": session_id,
                    "remote_path": remote,
                    "local_path": local_path,
                });
                let resp = conn.call("sftp_get", params)?;
                out.print_sftp_transfer(&resp, "Downloaded");
            }
            SftpAction::Put {
                session,
                local,
                remote,
            } => {
                return_if_incompatible(&compatibility)?;
                emit_compatibility_notice(out, cli.quiet, compatibility.warning.as_deref());
                let session_id = resolve_session_id(&mut conn, session)?;
                let params = serde_json::json!({
                    "session_id": session_id,
                    "local_path": local,
                    "remote_path": remote,
                });
                let resp = conn.call("sftp_put", params)?;
                out.print_sftp_transfer(&resp, "Uploaded");
            }
        },
        Commands::Import { action } => match action {
            ImportAction::List => {
                return_if_incompatible(&compatibility)?;
                emit_compatibility_notice(out, cli.quiet, compatibility.warning.as_deref());
                let resp = conn.call("import_list", serde_json::json!({}))?;
                out.print_import_list(&resp);
            }
            ImportAction::Add { aliases, all } => {
                return_if_incompatible(&compatibility)?;
                emit_compatibility_notice(out, cli.quiet, compatibility.warning.as_deref());
                // If --all, first fetch available hosts and use all non-imported aliases
                let aliases_to_import = if *all {
                    let hosts = conn.call("import_list", serde_json::json!({}))?;
                    let items = hosts.as_array().ok_or("Invalid import list")?;
                    items
                        .iter()
                        .filter(|h| {
                            !h.get("already_imported")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false)
                        })
                        .filter_map(|h| h.get("alias").and_then(|v| v.as_str()).map(String::from))
                        .collect::<Vec<_>>()
                } else {
                    aliases.clone()
                };

                if aliases_to_import.is_empty() {
                    if *all {
                        if !cli.quiet {
                            eprintln!("All hosts are already imported.");
                        }
                    } else {
                        return Err(CliError::usage(
                            "No aliases specified. Usage: oxt import add <alias1> <alias2> ... or --all",
                        ));
                    }
                    return Ok(ExitCode::Success);
                }

                let resp = conn.call(
                    "import_hosts",
                    serde_json::json!({ "aliases": aliases_to_import }),
                )?;
                out.print_import_result(&resp);
            }
        },
        Commands::Open { path } => {
            return_if_incompatible(&compatibility)?;
            emit_compatibility_notice(out, cli.quiet, compatibility.warning.as_deref());
            let dir = path.clone().unwrap_or_else(|| {
                std::env::current_dir()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| ".".to_string())
            });
            let resp = conn.call("open_tab", serde_json::json!({ "path": dir }))?;
            out.print_json(&resp);
        }
        Commands::Focus { latest, target } => {
            return_if_incompatible(&compatibility)?;
            emit_compatibility_notice(out, cli.quiet, compatibility.warning.as_deref());
            if *latest {
                let latest_target =
                    select_latest_focusable_target(&fetch_focusable_targets(&mut conn)?)?;
                if should_emit_human_guidance(out, cli.quiet) {
                    eprintln!(
                        "Focusing latest {}: {}",
                        latest_target.kind.label(),
                        latest_target.label
                    );
                }
                let resp = conn.call(
                    "focus_tab",
                    serde_json::json!({ "target": latest_target.id }),
                )?;
                out.print_json(&resp);
                return Ok(ExitCode::Success);
            }
            match target {
                Some(t) => {
                    let focus_target = match resolve_focusable_target(&mut conn, t) {
                        Ok(resolved) => resolved.id,
                        Err(error) if error.code == "not_found" => t.to_string(),
                        Err(error) => return Err(error),
                    };
                    let resp =
                        conn.call("focus_tab", serde_json::json!({ "target": focus_target }))?;
                    out.print_json(&resp);
                }
                None => {
                    // No target → list all focusable targets
                    let sessions = conn
                        .call("list_sessions", serde_json::json!({}))
                        .unwrap_or(serde_json::json!([]));
                    let locals = conn
                        .call("list_local_terminals", serde_json::json!({}))
                        .unwrap_or(serde_json::json!([]));
                    let ssh_items = sessions.as_array().map(|a| a.as_slice()).unwrap_or(&[]);
                    let local_items = locals.as_array().map(|a| a.as_slice()).unwrap_or(&[]);
                    let total = ssh_items.len() + local_items.len();

                    match total {
                        0 => {
                            return Err(CliError::not_found("No active tabs to focus."));
                        }
                        1 => {
                            // Auto-focus the single tab
                            let (id, label) = if let Some(s) = ssh_items.first() {
                                (
                                    s.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                                    s.get("name").and_then(|v| v.as_str()).unwrap_or("?"),
                                )
                            } else {
                                let l = &local_items[0];
                                (
                                    l.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                                    l.get("shell_name").and_then(|v| v.as_str()).unwrap_or("?"),
                                )
                            };
                            if should_emit_human_guidance(out, cli.quiet) {
                                eprintln!("Auto-focusing: {label}");
                            }
                            let resp =
                                conn.call("focus_tab", serde_json::json!({ "target": id }))?;
                            out.print_json(&resp);
                        }
                        _ => {
                            if should_emit_human_guidance(out, cli.quiet) {
                                eprintln!("Multiple active tabs — specify a target:\n");
                                if !ssh_items.is_empty() {
                                    eprintln!("  SSH Sessions:");
                                    out.print_sessions(&sessions);
                                }
                                if !local_items.is_empty() {
                                    eprintln!("\n  Local Terminals:");
                                    out.print_local_terminals(&locals);
                                }
                                eprintln!("\nUsage: oxt focus <NAME-OR-ID>");
                                eprintln!("  Matches: session name/ID, shell name, or tab title");
                            }
                            return Err(CliError::usage(
                                "Multiple active tabs — specify a target.",
                            ));
                        }
                    }
                }
            }
        }
        Commands::Ping => {
            emit_compatibility_notice(
                out,
                cli.quiet,
                compatibility
                    .error
                    .as_deref()
                    .or(compatibility.warning.as_deref()),
            );
            let resp = conn.call("ping", serde_json::json!({}))?;
            out.print_json(&resp);
        }
        Commands::Attach {
            latest,
            readonly,
            target,
        } => {
            return_if_incompatible(&compatibility)?;
            emit_compatibility_notice(out, cli.quiet, compatibility.warning.as_deref());
            run_attach(
                &mut conn,
                out,
                target.as_deref(),
                *latest,
                *readonly,
                cli.quiet,
            )?;
            return Ok(ExitCode::Success);
        }
        Commands::Version | Commands::Completions { .. } | Commands::Doctor => unreachable!(),
    }

    Ok(ExitCode::Success)
}

fn build_doctor_report(cli: &Cli) -> DoctorReport {
    let mut report = DoctorReport::new();

    let current_exe = match std::env::current_exe() {
        Ok(path) => {
            report.binary_path = Some(path.display().to_string());
            report.push(doctor_check(
                "binary_path",
                "CLI binary path",
                DoctorStatus::Ok,
                format!("Running {}", path.display()),
                None,
            ));
            Some(path)
        }
        Err(error) => {
            report.push(doctor_check(
                "binary_path",
                "CLI binary path",
                DoctorStatus::Fail,
                "Unable to determine the current CLI binary path".to_string(),
                Some(error.to_string()),
            ));
            None
        }
    };

    report.push(build_path_check(current_exe.as_deref()));

    let endpoint = match connect::resolve_endpoint(cli.socket.as_deref()) {
        Ok(endpoint) => {
            let display = endpoint.display();
            report.endpoint.value = Some(display.clone());
            report.endpoint.source = Some(endpoint.source().description().to_string());
            let status = if endpoint.source().is_override() {
                DoctorStatus::Warn
            } else {
                DoctorStatus::Ok
            };
            let summary = if endpoint.source().is_override() {
                format!("Using {}: {display}", endpoint.source().description())
            } else {
                format!("Using default endpoint: {display}")
            };
            report.push(doctor_check(
                "endpoint_resolution",
                "Endpoint resolution",
                status,
                summary,
                None,
            ));
            Some(endpoint)
        }
        Err(error) => {
            report.push(doctor_check(
                "endpoint_resolution",
                "Endpoint resolution",
                DoctorStatus::Fail,
                "Unable to resolve the IPC endpoint".to_string(),
                Some(error),
            ));
            None
        }
    };

    if let Some(endpoint) = endpoint.as_ref() {
        let inspection = connect::inspect_endpoint(endpoint);
        report.push(build_endpoint_presence_check(endpoint, &inspection));
        report.push(build_endpoint_ownership_check(&inspection));

        match connect::IpcConnection::connect_resolved(endpoint, cli.timeout) {
            Ok(mut conn) => match conn.call("status", serde_json::json!({})) {
                Ok(status) => {
                    let version = status
                        .get("version")
                        .and_then(|value| value.as_str())
                        .unwrap_or("unknown");
                    let pid = status.get("pid").and_then(|value| value.as_u64());
                    let summary = match pid {
                        Some(pid) => format!("Connected to OxideTerm {version} (pid {pid})"),
                        None => format!("Connected to OxideTerm {version}"),
                    };
                    report.push(doctor_check(
                        "gui_connectivity",
                        "GUI connectivity",
                        DoctorStatus::Ok,
                        summary,
                        None,
                    ));
                    report.push(build_compatibility_check(&status));
                }
                Err(error) => {
                    report.push(doctor_check(
                        "gui_connectivity",
                        "GUI connectivity",
                        DoctorStatus::Fail,
                        "Connected to the endpoint, but the GUI did not return status".to_string(),
                        Some(error),
                    ));
                    report.push(doctor_check(
                        "cli_api_compatibility",
                        "CLI API compatibility",
                        DoctorStatus::Warn,
                        "Compatibility was not checked because status probing failed".to_string(),
                        None,
                    ));
                }
            },
            Err(error) => {
                report.push(doctor_check(
                    "gui_connectivity",
                    "GUI connectivity",
                    DoctorStatus::Fail,
                    "Failed to connect to the OxideTerm GUI".to_string(),
                    Some(error),
                ));
                report.push(doctor_check(
                    "cli_api_compatibility",
                    "CLI API compatibility",
                    DoctorStatus::Warn,
                    "Compatibility was not checked because the GUI is unreachable".to_string(),
                    None,
                ));
            }
        }
    } else {
        report.push(doctor_check(
            "endpoint_presence",
            "Endpoint presence",
            DoctorStatus::Warn,
            "Endpoint presence was not checked because endpoint resolution failed".to_string(),
            None,
        ));
        report.push(doctor_check(
            "endpoint_ownership",
            "Endpoint ownership",
            DoctorStatus::Warn,
            "Endpoint ownership was not checked because endpoint resolution failed".to_string(),
            None,
        ));
        report.push(doctor_check(
            "gui_connectivity",
            "GUI connectivity",
            DoctorStatus::Warn,
            "GUI connectivity was not checked because endpoint resolution failed".to_string(),
            None,
        ));
        report.push(doctor_check(
            "cli_api_compatibility",
            "CLI API compatibility",
            DoctorStatus::Warn,
            "Compatibility was not checked because endpoint resolution failed".to_string(),
            None,
        ));
    }

    report
}

fn doctor_check(
    id: &'static str,
    title: &'static str,
    status: DoctorStatus,
    summary: String,
    detail: Option<String>,
) -> DoctorCheck {
    DoctorCheck {
        id,
        title,
        status,
        summary,
        detail,
    }
}

fn build_path_check(current_exe: Option<&std::path::Path>) -> DoctorCheck {
    let Some(current_exe) = current_exe else {
        return doctor_check(
            "path_lookup",
            "PATH lookup",
            DoctorStatus::Warn,
            "PATH lookup was skipped because the current executable path is unavailable"
                .to_string(),
            None,
        );
    };

    let binary_name = cli_binary_name();
    match find_command_in_path(binary_name) {
        Ok(Some(candidate)) => {
            if candidate.broken_symlink {
                return doctor_check(
                    "path_lookup",
                    "PATH lookup",
                    DoctorStatus::Warn,
                    format!(
                        "{binary_name} resolves to a broken symlink: {}",
                        candidate.path.display()
                    ),
                    Some(format!(
                        "Reinstall the bundled CLI or remove the stale PATH entry. Expected install path: {}",
                        expected_cli_install_path()
                    )),
                );
            }

            if !candidate.executable {
                return doctor_check(
                    "path_lookup",
                    "PATH lookup",
                    DoctorStatus::Warn,
                    format!(
                        "{binary_name} resolves to a non-executable file: {}",
                        candidate.path.display()
                    ),
                    Some(
                        "Fix the file permissions or remove the conflicting PATH entry before reinstalling the CLI"
                            .to_string(),
                    ),
                );
            }

            let current_canonical = current_exe
                .canonicalize()
                .unwrap_or_else(|_| current_exe.to_path_buf());
            let found_canonical = candidate
                .path
                .canonicalize()
                .unwrap_or_else(|_| candidate.path.clone());
            if current_canonical == found_canonical {
                doctor_check(
                    "path_lookup",
                    "PATH lookup",
                    DoctorStatus::Ok,
                    format!("{binary_name} resolves to {}", candidate.path.display()),
                    None,
                )
            } else {
                doctor_check(
                    "path_lookup",
                    "PATH lookup",
                    DoctorStatus::Warn,
                    format!(
                        "{binary_name} resolves to a different binary: {}",
                        candidate.path.display()
                    ),
                    Some(format!(
                        "Current executable is {}. Reinstall the bundled CLI or adjust PATH order. Expected install path: {}",
                        current_exe.display(),
                        expected_cli_install_path()
                    )),
                )
            }
        }
        Ok(None) => doctor_check(
            "path_lookup",
            "PATH lookup",
            DoctorStatus::Warn,
            format!("{binary_name} was not found in PATH"),
            Some(format!(
                "Install the bundled CLI or add its directory to PATH. Expected install path: {}",
                expected_cli_install_path()
            )),
        ),
        Err(error) => doctor_check(
            "path_lookup",
            "PATH lookup",
            DoctorStatus::Warn,
            "Failed to inspect PATH".to_string(),
            Some(error),
        ),
    }
}

fn build_endpoint_presence_check(
    endpoint: &connect::EndpointResolution,
    inspection: &connect::EndpointInspection,
) -> DoctorCheck {
    #[cfg(windows)]
    {
        let _ = inspection;
        return doctor_check(
            "endpoint_presence",
            "Endpoint presence",
            DoctorStatus::Warn,
            format!(
                "Named pipe presence is not statically probed on Windows: {}",
                endpoint.display()
            ),
            Some("Use the GUI connectivity check below as the authoritative signal".to_string()),
        );
    }

    if let Some(error) = inspection.metadata_error.as_ref() {
        return doctor_check(
            "endpoint_presence",
            "Endpoint presence",
            DoctorStatus::Fail,
            format!("Unable to inspect endpoint: {}", endpoint.display()),
            Some(error.clone()),
        );
    }

    if !inspection.exists {
        return doctor_check(
            "endpoint_presence",
            "Endpoint presence",
            DoctorStatus::Fail,
            format!("Endpoint not found: {}", endpoint.display()),
            Some("Start OxideTerm, or pass --socket/--pipe if you are targeting a non-default endpoint".to_string()),
        );
    }

    #[cfg(unix)]
    {
        if inspection.kind.as_deref() != Some("socket") {
            return doctor_check(
                "endpoint_presence",
                "Endpoint presence",
                DoctorStatus::Fail,
                format!(
                    "Endpoint exists but is not a Unix socket: {}",
                    endpoint.display()
                ),
                inspection
                    .kind
                    .as_ref()
                    .map(|kind| format!("Detected endpoint type: {kind}")),
            );
        }
    }

    doctor_check(
        "endpoint_presence",
        "Endpoint presence",
        DoctorStatus::Ok,
        format!("Endpoint exists: {}", endpoint.display()),
        None,
    )
}

fn build_endpoint_ownership_check(inspection: &connect::EndpointInspection) -> DoctorCheck {
    #[cfg(unix)]
    {
        if !inspection.exists {
            return doctor_check(
                "endpoint_ownership",
                "Endpoint ownership",
                DoctorStatus::Warn,
                "Ownership was not checked because the endpoint is missing".to_string(),
                None,
            );
        }

        if let Some(error) = inspection.metadata_error.as_ref() {
            return doctor_check(
                "endpoint_ownership",
                "Endpoint ownership",
                DoctorStatus::Fail,
                "Unable to inspect endpoint ownership".to_string(),
                Some(error.clone()),
            );
        }

        if inspection.kind.as_deref() != Some("socket") {
            return doctor_check(
                "endpoint_ownership",
                "Endpoint ownership",
                DoctorStatus::Warn,
                "Ownership was not checked because the endpoint is not a Unix socket".to_string(),
                inspection
                    .kind
                    .as_ref()
                    .map(|kind| format!("Detected endpoint type: {kind}")),
            );
        }

        match (
            inspection.ownership_matches,
            inspection.owner_uid,
            inspection.current_uid,
        ) {
            (Some(true), Some(owner_uid), Some(current_uid)) => doctor_check(
                "endpoint_ownership",
                "Endpoint ownership",
                DoctorStatus::Ok,
                format!("Endpoint owner uid {owner_uid} matches current uid {current_uid}"),
                None,
            ),
            (Some(false), Some(owner_uid), Some(current_uid)) => doctor_check(
                "endpoint_ownership",
                "Endpoint ownership",
                DoctorStatus::Fail,
                format!("Endpoint owner uid {owner_uid} does not match current uid {current_uid}"),
                Some("This can indicate a stale socket or a local security issue".to_string()),
            ),
            _ => doctor_check(
                "endpoint_ownership",
                "Endpoint ownership",
                DoctorStatus::Warn,
                "Ownership information is unavailable for this endpoint".to_string(),
                None,
            ),
        }
    }

    #[cfg(windows)]
    {
        let _ = inspection;
        doctor_check(
            "endpoint_ownership",
            "Endpoint ownership",
            DoctorStatus::Warn,
            "Named pipe ownership checks are not yet implemented on Windows".to_string(),
            None,
        )
    }
}

fn build_compatibility_check(status: &serde_json::Value) -> DoctorCheck {
    let compatibility = check_compatibility(status.clone());
    if let Some(error) = compatibility.error {
        return doctor_check(
            "cli_api_compatibility",
            "CLI API compatibility",
            DoctorStatus::Fail,
            error,
            None,
        );
    }
    if let Some(warning) = compatibility.warning {
        return doctor_check(
            "cli_api_compatibility",
            "CLI API compatibility",
            DoctorStatus::Warn,
            warning,
            None,
        );
    }

    let server_version = status
        .pointer("/cli_api/version")
        .and_then(|value| value.as_u64())
        .unwrap_or(CLI_API_CURRENT_VERSION);
    let server_min = status
        .pointer("/cli_api/min_supported")
        .and_then(|value| value.as_u64())
        .unwrap_or(server_version);

    doctor_check(
        "cli_api_compatibility",
        "CLI API compatibility",
        DoctorStatus::Ok,
        format!(
            "CLI API ranges overlap: client {}-{}, server {}-{}",
            CLI_API_MIN_SUPPORTED_VERSION, CLI_API_CURRENT_VERSION, server_min, server_version
        ),
        None,
    )
}

fn expected_cli_install_path() -> String {
    #[cfg(unix)]
    {
        if let Some(home) = dirs::home_dir() {
            return home
                .join(".local")
                .join("bin")
                .join("oxt")
                .display()
                .to_string();
        }
        "/usr/local/bin/oxt".to_string()
    }

    #[cfg(windows)]
    {
        if let Some(local_app_data) = dirs::data_local_dir() {
            return local_app_data
                .join("OxideTerm")
                .join("bin")
                .join("oxt.exe")
                .display()
                .to_string();
        }
        "oxt.exe".to_string()
    }
}

fn cli_binary_name() -> &'static str {
    #[cfg(windows)]
    {
        "oxt.exe"
    }

    #[cfg(not(windows))]
    {
        "oxt"
    }
}

fn find_command_in_path(binary_name: &str) -> Result<Option<PathCandidate>, String> {
    let path_var = std::env::var_os("PATH").ok_or("PATH is not set")?;

    #[cfg(windows)]
    {
        let mut candidates = vec![binary_name.to_string()];
        if !binary_name.contains('.') {
            if let Some(pathext) = std::env::var_os("PATHEXT") {
                for ext in std::env::split_paths(&std::path::PathBuf::from(pathext)) {
                    let ext = ext.to_string_lossy();
                    candidates.push(format!("{binary_name}{ext}"));
                }
            }
        }

        for dir in std::env::split_paths(&path_var) {
            for candidate in &candidates {
                let path = dir.join(candidate);
                if path.is_file() {
                    return Ok(Some(PathCandidate {
                        path,
                        executable: true,
                        broken_symlink: false,
                    }));
                }
            }
        }
    }

    #[cfg(not(windows))]
    {
        use std::os::unix::fs::{FileTypeExt, PermissionsExt};

        for dir in std::env::split_paths(&path_var) {
            let path = dir.join(binary_name);
            let Ok(metadata) = std::fs::symlink_metadata(&path) else {
                continue;
            };

            if metadata.file_type().is_symlink() && !path.exists() {
                return Ok(Some(PathCandidate {
                    path,
                    executable: false,
                    broken_symlink: true,
                }));
            }

            let executable = std::fs::metadata(&path)
                .map(|target| target.is_file() && (target.permissions().mode() & 0o111 != 0))
                .unwrap_or(false);

            if metadata.is_file()
                || metadata.file_type().is_symlink()
                || metadata.file_type().is_socket()
            {
                return Ok(Some(PathCandidate {
                    path,
                    executable,
                    broken_symlink: false,
                }));
            }
        }
    }

    Ok(None)
}

fn fetch_compatibility_report(
    conn: &mut connect::IpcConnection,
) -> Result<CompatibilityReport, CliError> {
    let status = conn.call("status", serde_json::json!({}))?;
    Ok(check_compatibility(status))
}

fn classify_error_message(message: String) -> CliError {
    let message_lower = message.to_ascii_lowercase();

    if message.contains("CLI API mismatch")
        || message.contains("does not expose CLI API compatibility metadata")
    {
        return CliError::compatibility(message);
    }
    if message_lower.contains("timed out")
        || message_lower.contains("timeout")
        || message.contains("[1003]")
    {
        return CliError::timeout(message);
    }
    if message_lower.contains("connection not found:")
        || message_lower.contains("session not found:")
        || message_lower.contains("no health tracker for session:")
        || message.starts_with("No active tabs to focus.")
        || message.starts_with("No active sessions to attach to.")
    {
        return CliError::not_found(message);
    }
    if message.starts_with("No prompt provided.")
        || message.starts_with("No aliases specified.")
        || message.starts_with("Multiple active tabs")
        || message.starts_with("Forward spec:")
        || message.starts_with("Dynamic forward spec:")
        || message.starts_with("Invalid ")
        || message.starts_with("Unclosed bracket")
    {
        return CliError::usage(message);
    }
    CliError::runtime(message)
}

fn compatibility_notice_text(
    out: &output::OutputMode,
    quiet: bool,
    message: Option<&str>,
) -> Option<String> {
    if !should_emit_human_guidance(out, quiet) {
        return None;
    }
    message.map(str::to_string)
}

fn should_emit_human_guidance(out: &output::OutputMode, quiet: bool) -> bool {
    !out.is_json() && !quiet
}

fn check_compatibility(status: serde_json::Value) -> CompatibilityReport {
    let server_version = status
        .get("version")
        .and_then(|value| value.as_str())
        .map(str::to_string);
    let server_api_version = status
        .pointer("/cli_api/version")
        .and_then(|value| value.as_u64());
    let server_api_min_supported = status
        .pointer("/cli_api/min_supported")
        .and_then(|value| value.as_u64());

    let error = match (server_api_version, server_api_min_supported) {
        (Some(server_api_version), Some(server_api_min_supported)) => {
            if protocol_ranges_overlap(
                CLI_API_MIN_SUPPORTED_VERSION,
                CLI_API_CURRENT_VERSION,
                server_api_min_supported,
                server_api_version,
            ) {
                None
            } else {
                Some(format!(
                    "CLI API mismatch: oxt {} supports CLI API {}-{}, but OxideTerm {} supports {}-{}. Update OxideTerm or reinstall the bundled CLI.",
                    env!("CARGO_PKG_VERSION"),
                    CLI_API_MIN_SUPPORTED_VERSION,
                    CLI_API_CURRENT_VERSION,
                    server_version.as_deref().unwrap_or("unknown"),
                    server_api_min_supported,
                    server_api_version,
                ))
            }
        }
        _ => Some(format!(
            "OxideTerm {} does not expose CLI API compatibility metadata. This CLI requires a newer app build with cli_api status support.",
            server_version.as_deref().unwrap_or("unknown"),
        )),
    };

    let warning = if error.is_none() {
        server_version.and_then(|version| {
            if version != env!("CARGO_PKG_VERSION") {
                Some(format!(
                    "CLI version {} is talking to OxideTerm {}. CLI API is compatible, but reinstalling the bundled CLI is recommended.",
                    env!("CARGO_PKG_VERSION"),
                    version,
                ))
            } else {
                None
            }
        })
    } else {
        None
    };

    CompatibilityReport {
        status,
        warning,
        error,
    }
}

fn protocol_ranges_overlap(
    client_min: u64,
    client_max: u64,
    server_min: u64,
    server_max: u64,
) -> bool {
    client_min <= server_max && server_min <= client_max
}

fn return_if_incompatible(report: &CompatibilityReport) -> Result<(), CliError> {
    if let Some(error) = &report.error {
        return Err(CliError::compatibility(error.clone()));
    }
    Ok(())
}

fn emit_compatibility_notice(out: &output::OutputMode, quiet: bool, message: Option<&str>) {
    if let Some(message) = compatibility_notice_text(out, quiet, message) {
        eprintln!("warning: {message}");
    }
}

fn watch_status(
    conn: &mut connect::IpcConnection,
    out: &output::OutputMode,
    quiet: bool,
    first_report: CompatibilityReport,
    interval_ms: u64,
) -> Result<(), CliError> {
    let mut report = first_report;
    let mut first_iteration = true;

    loop {
        if !first_iteration && !out.is_json() {
            println!();
        }
        out.print_status(&report.status);
        if first_iteration {
            emit_compatibility_notice(
                out,
                quiet,
                report.error.as_deref().or(report.warning.as_deref()),
            );
        }

        sleep_poll_interval(interval_ms);
        report = fetch_compatibility_report(conn)?;
        first_iteration = false;
    }
}

fn watch_health(
    conn: &mut connect::IpcConnection,
    out: &output::OutputMode,
    session_id: Option<&str>,
    interval_ms: u64,
) -> Result<(), CliError> {
    let mut first_iteration = true;
    loop {
        let params = match session_id {
            Some(id) => serde_json::json!({ "session_id": id }),
            None => serde_json::json!({}),
        };
        let resp = conn.call("health", params)?;
        if !first_iteration && !out.is_json() {
            println!();
        }
        out.print_health(&resp, session_id.is_some());

        sleep_poll_interval(interval_ms);
        first_iteration = false;
    }
}

fn sleep_poll_interval(interval_ms: u64) {
    std::thread::sleep(std::time::Duration::from_millis(interval_ms));
}

fn wait_for_connected_session(
    conn: &mut connect::IpcConnection,
    connect_response: &serde_json::Value,
    interval_ms: u64,
    wait_timeout_ms: u64,
) -> Result<serde_json::Value, CliError> {
    let connection_id = connect_response
        .get("connection_id")
        .and_then(|value| value.as_str())
        .ok_or_else(|| CliError::runtime("Connect response did not include connection_id"))?;
    let connection_name = connect_response
        .get("name")
        .and_then(|value| value.as_str())
        .unwrap_or(connection_id);
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(wait_timeout_ms);

    loop {
        let sessions = conn.call("list_sessions", serde_json::json!({}))?;
        let items = sessions
            .as_array()
            .ok_or_else(|| CliError::runtime("Invalid session list from server"))?;

        if let Some(session) = items.iter().find(|session| {
            session
                .get("connection_id")
                .and_then(|value| value.as_str())
                == Some(connection_id)
        }) {
            let mut result = connect_response.clone();
            result["waited"] = serde_json::json!(true);
            if let Some(session_id) = session.get("id") {
                result["session_id"] = session_id.clone();
            }
            if let Some(state) = session.get("state") {
                result["session_state"] = state.clone();
            }
            return Ok(result);
        }

        if std::time::Instant::now() >= deadline {
            return Err(CliError::timeout(format!(
                "Timed out waiting for connection {connection_name} to appear in the active session list"
            )));
        }

        sleep_poll_interval(interval_ms);
    }
}

fn build_session_inspect(
    conn: &mut connect::IpcConnection,
    target: &str,
) -> Result<serde_json::Value, CliError> {
    let sessions = conn.call("list_sessions", serde_json::json!({}))?;
    let items = sessions
        .as_array()
        .ok_or_else(|| CliError::runtime("Invalid session list from server"))?;
    let session = select_session_for_inspect(items, target)?;
    let session_id = session
        .get("id")
        .and_then(|value| value.as_str())
        .ok_or_else(|| CliError::runtime("Session entry is missing id"))?;

    let forwards = conn.call(
        "list_forwards",
        serde_json::json!({ "session_id": session_id }),
    )?;

    let health_result = match conn.call("health", serde_json::json!({ "session_id": session_id })) {
        Ok(health) => (health, None),
        Err(error) if error.contains("No health tracker for session:") => {
            (serde_json::Value::Null, Some(error))
        }
        Err(error) => return Err(error.into()),
    };

    let connection = if let Some(connection_id) = session
        .get("connection_id")
        .and_then(|value| value.as_str())
    {
        let connections = conn.call("list_active_connections", serde_json::json!({}))?;
        let items = connections
            .as_array()
            .ok_or_else(|| CliError::runtime("Invalid active connection list from server"))?;

        items
            .iter()
            .find(|entry| entry.get("id").and_then(|value| value.as_str()) == Some(connection_id))
            .cloned()
            .unwrap_or(serde_json::Value::Null)
    } else {
        serde_json::Value::Null
    };

    Ok(serde_json::json!({
        "session": session,
        "connection": connection,
        "health": health_result.0,
        "health_error": health_result.1,
        "forwards": forwards,
    }))
}

fn select_session_for_inspect(
    sessions: &[serde_json::Value],
    target: &str,
) -> Result<serde_json::Value, CliError> {
    if let Some(session) = sessions
        .iter()
        .find(|session| session.get("id").and_then(|value| value.as_str()) == Some(target))
    {
        return Ok(session.clone());
    }

    let name_matches: Vec<&serde_json::Value> = sessions
        .iter()
        .filter(|session| session.get("name").and_then(|value| value.as_str()) == Some(target))
        .collect();
    if name_matches.len() == 1 {
        return Ok(name_matches[0].clone());
    }
    if name_matches.len() > 1 {
        return Err(CliError::usage(format!(
            "Multiple active sessions match name: {target}. Use the full session ID."
        )));
    }

    let prefix_matches: Vec<&serde_json::Value> = sessions
        .iter()
        .filter(|session| {
            session
                .get("id")
                .and_then(|value| value.as_str())
                .map(|id| id.starts_with(target))
                .unwrap_or(false)
        })
        .collect();
    if prefix_matches.len() == 1 {
        return Ok(prefix_matches[0].clone());
    }
    if prefix_matches.len() > 1 {
        return Err(CliError::usage(format!(
            "Multiple active sessions match ID prefix: {target}. Use a longer prefix or the full session ID."
        )));
    }

    Err(CliError::not_found(format!("Session not found: {target}")))
}

/// Parse a forward spec like `8080:localhost:80` or `0.0.0.0:8080:localhost:80`
/// Also supports IPv6 addresses in brackets: `[::1]:8080:localhost:80`
/// For dynamic forwards, spec is just `[bind_addr:]bind_port`
fn parse_forward_spec(spec: &str, fwd_type: &str) -> Result<(String, u16, String, u16), String> {
    // Tokenize respecting bracketed IPv6 addresses
    let tokens = tokenize_spec(spec)?;

    if fwd_type == "dynamic" {
        // Dynamic: [bind_addr:]bind_port
        return match tokens.len() {
            1 => {
                let port: u16 = tokens[0]
                    .parse()
                    .map_err(|_| format!("Invalid port: {}", tokens[0]))?;
                Ok(("127.0.0.1".to_string(), port, String::new(), 0))
            }
            2 => {
                let port: u16 = tokens[1]
                    .parse()
                    .map_err(|_| format!("Invalid port: {}", tokens[1]))?;
                Ok((tokens[0].clone(), port, String::new(), 0))
            }
            _ => Err("Dynamic forward spec: [bind_addr:]bind_port".to_string()),
        };
    }

    // local/remote: [bind_addr:]bind_port:target_host:target_port
    match tokens.len() {
        3 => {
            let bind_port: u16 = tokens[0]
                .parse()
                .map_err(|_| format!("Invalid bind port: {}", tokens[0]))?;
            let target_port: u16 = tokens[2]
                .parse()
                .map_err(|_| format!("Invalid target port: {}", tokens[2]))?;
            Ok((
                "127.0.0.1".to_string(),
                bind_port,
                tokens[1].clone(),
                target_port,
            ))
        }
        4 => {
            let bind_port: u16 = tokens[1]
                .parse()
                .map_err(|_| format!("Invalid bind port: {}", tokens[1]))?;
            let target_port: u16 = tokens[3]
                .parse()
                .map_err(|_| format!("Invalid target port: {}", tokens[3]))?;
            Ok((tokens[0].clone(), bind_port, tokens[2].clone(), target_port))
        }
        _ => Err("Forward spec: [bind_addr:]bind_port:target_host:target_port".to_string()),
    }
}

/// Tokenize a colon-separated spec, respecting `[...]` for IPv6 addresses.
/// e.g. `[::1]:8080:localhost:80` → `["::1", "8080", "localhost", "80"]`
fn tokenize_spec(spec: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut chars = spec.chars().peekable();

    while chars.peek().is_some() {
        if chars.peek() == Some(&'[') {
            // Bracketed token (IPv6)
            chars.next(); // consume '['
            let mut token = String::new();
            loop {
                match chars.next() {
                    Some(']') => break,
                    Some(c) => token.push(c),
                    None => return Err("Unclosed bracket in forward spec".to_string()),
                }
            }
            tokens.push(token);
            // Consume the following ':' separator if present
            if chars.peek() == Some(&':') {
                chars.next();
            }
        } else {
            // Regular token until next ':'
            let mut token = String::new();
            while let Some(&c) = chars.peek() {
                if c == ':' {
                    chars.next(); // consume ':'
                    break;
                }
                token.push(c);
                chars.next();
            }
            tokens.push(token);
        }
    }

    Ok(tokens)
}

/// Resolve a session target (name or ID) to a session ID.
fn resolve_session_id(conn: &mut connect::IpcConnection, target: &str) -> Result<String, String> {
    let sessions = conn.call("list_sessions", serde_json::json!({}))?;
    let items = sessions.as_array().ok_or("Invalid session list")?;

    // Try exact ID match
    if let Some(s) = items
        .iter()
        .find(|s| s.get("id").and_then(|v| v.as_str()) == Some(target))
    {
        return Ok(s["id"].as_str().unwrap().to_string());
    }
    // Try name match
    if let Some(s) = items
        .iter()
        .find(|s| s.get("name").and_then(|v| v.as_str()) == Some(target))
    {
        return Ok(s["id"].as_str().unwrap().to_string());
    }
    // Try partial ID match
    if let Some(s) = items.iter().find(|s| {
        s.get("id")
            .and_then(|v| v.as_str())
            .map(|id| id.starts_with(target))
            .unwrap_or(false)
    }) {
        return Ok(s["id"].as_str().unwrap().to_string());
    }

    Err(format!("Session not found: {target}"))
}

fn is_terminal_stdin() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal()
}

fn read_stdin() -> Result<String, String> {
    let mut stdin = std::io::stdin();
    read_stdin_from_reader(&mut stdin, 512 * 1024)
}

fn read_stdin_from_reader<R: std::io::Read>(
    reader: &mut R,
    max_size: usize,
) -> Result<String, String> {
    use std::io::Read;

    let mut buf = String::new();
    reader
        .take((max_size + 1) as u64)
        .read_to_string(&mut buf)
        .map_err(|e| format!("Failed to read stdin: {e}"))?;

    if buf.len() > max_size {
        return Err(format!(
            "Piped stdin exceeds the {} KB limit",
            max_size / 1024
        ));
    }

    Ok(buf)
}

#[cfg(any(windows, test))]
enum StdinCoalesceEvent {
    Bytes(Vec<u8>),
    Closed,
}

#[cfg(any(windows, test))]
fn coalesce_stdin_chunks<F, E>(
    mut raw_input: Vec<u8>,
    max_size: usize,
    idle_window: std::time::Duration,
    mut recv_next: F,
) -> Result<(Vec<u8>, bool), E>
where
    F: FnMut(std::time::Duration) -> Result<Option<StdinCoalesceEvent>, E>,
{
    let mut stdin_closed = false;

    while raw_input.len() < max_size {
        match recv_next(idle_window)? {
            Some(StdinCoalesceEvent::Bytes(more)) => raw_input.extend(more),
            Some(StdinCoalesceEvent::Closed) => {
                stdin_closed = true;
                break;
            }
            None => break,
        }
    }

    Ok((raw_input, stdin_closed))
}

/// Render markdown text to the terminal using termimad.
fn render_markdown(text: &str) {
    let skin = termimad::MadSkin::default();
    // termimad writes ANSI-colored markdown to the terminal
    skin.print_text(text);
}

fn should_render_ai_markdown(raw: bool, out: &output::OutputMode) -> bool {
    !raw && matches!(out, output::OutputMode::Human)
}

/// Print the conversation ID from the response for `--continue` use.
fn print_conversation_id(resp: &serde_json::Value, quiet: bool) {
    if quiet {
        return;
    }
    if let Some(cid) = resp.get("conversation_id").and_then(|v| v.as_str()) {
        eprintln!("\n\x1b[2mConversation: {cid}\x1b[0m");
        eprintln!("\x1b[2mContinue with: oxt ask --continue {cid} \"your follow-up\"\x1b[0m");
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum FocusableKind {
    Ssh,
    Local,
}

impl FocusableKind {
    fn label(&self) -> &'static str {
        match self {
            Self::Ssh => "SSH session",
            Self::Local => "local terminal",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FocusableTarget {
    id: String,
    label: String,
    shell_id: Option<String>,
    created_at: Option<String>,
    kind: FocusableKind,
}

fn fetch_focusable_targets(
    conn: &mut connect::IpcConnection,
) -> Result<Vec<FocusableTarget>, CliError> {
    let sessions = conn.call("list_sessions", serde_json::json!({}))?;
    let ssh_items = sessions
        .as_array()
        .ok_or_else(|| CliError::runtime("Invalid session list from server"))?;

    let locals = conn.call("list_local_terminals", serde_json::json!({}))?;
    let local_items = locals
        .as_array()
        .ok_or_else(|| CliError::runtime("Invalid local terminal list from server"))?;

    let mut targets = Vec::with_capacity(ssh_items.len() + local_items.len());

    for session in ssh_items {
        let id = session
            .get("id")
            .and_then(|value| value.as_str())
            .ok_or_else(|| CliError::runtime("Session entry is missing id"))?;
        let label = session
            .get("name")
            .and_then(|value| value.as_str())
            .unwrap_or(id);
        targets.push(FocusableTarget {
            id: id.to_string(),
            label: label.to_string(),
            shell_id: None,
            created_at: session
                .get("created_at")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned),
            kind: FocusableKind::Ssh,
        });
    }

    for local in local_items {
        let id = local
            .get("id")
            .and_then(|value| value.as_str())
            .ok_or_else(|| CliError::runtime("Local terminal entry is missing id"))?;
        let label = local
            .get("shell_name")
            .and_then(|value| value.as_str())
            .unwrap_or(id);
        targets.push(FocusableTarget {
            id: id.to_string(),
            label: label.to_string(),
            shell_id: local
                .get("shell_id")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned),
            created_at: local
                .get("created_at")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned),
            kind: FocusableKind::Local,
        });
    }

    Ok(targets)
}

fn resolve_focusable_target(
    conn: &mut connect::IpcConnection,
    target: &str,
) -> Result<FocusableTarget, CliError> {
    let targets = fetch_focusable_targets(conn)?;
    resolve_focusable_target_from_targets(&targets, target)
}

fn resolve_focusable_target_from_targets(
    targets: &[FocusableTarget],
    target: &str,
) -> Result<FocusableTarget, CliError> {
    if let Some(found) = targets.iter().find(|item| item.id == target) {
        return Ok(found.clone());
    }

    let exact_matches: Vec<&FocusableTarget> = targets
        .iter()
        .filter(|item| item.label == target || item.shell_id.as_deref() == Some(target))
        .collect();
    if exact_matches.len() == 1 {
        return Ok(exact_matches[0].clone());
    }
    if exact_matches.len() > 1 {
        return Err(CliError::usage(format!(
            "Multiple active tabs match target: {target}. Use the full session ID."
        )));
    }

    let prefix_matches: Vec<&FocusableTarget> = targets
        .iter()
        .filter(|item| item.id.starts_with(target))
        .collect();
    if prefix_matches.len() == 1 {
        return Ok(prefix_matches[0].clone());
    }
    if prefix_matches.len() > 1 {
        return Err(CliError::usage(format!(
            "Multiple active tabs match ID prefix: {target}. Use a longer prefix or the full session ID."
        )));
    }

    Err(CliError::not_found(format!("Session not found: {target}")))
}

fn select_latest_focusable_target(
    targets: &[FocusableTarget],
) -> Result<FocusableTarget, CliError> {
    targets
        .iter()
        .max_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.id.cmp(&right.id))
        })
        .cloned()
        .ok_or_else(|| CliError::not_found("No active tabs to focus."))
}

/// Main entry point for `oxt attach`.
fn run_attach(
    conn: &mut connect::IpcConnection,
    out: &output::OutputMode,
    target: Option<&str>,
    latest: bool,
    readonly: bool,
    quiet: bool,
) -> Result<(), CliError> {
    let focusable_targets = fetch_focusable_targets(conn)?;

    let resolved_target = match target {
        Some(t) => resolve_focusable_target_from_targets(&focusable_targets, t)?,
        None if latest => select_latest_focusable_target(&focusable_targets)
            .map_err(|_| CliError::not_found("No active sessions to attach to."))?,
        None => {
            let sessions = conn
                .call("list_sessions", serde_json::json!({}))
                .unwrap_or(serde_json::json!([]));
            let locals = conn
                .call("list_local_terminals", serde_json::json!({}))
                .unwrap_or(serde_json::json!([]));
            let ssh_items = sessions.as_array().map(|a| a.as_slice()).unwrap_or(&[]);
            let local_items = locals.as_array().map(|a| a.as_slice()).unwrap_or(&[]);
            let total = ssh_items.len() + local_items.len();

            if total == 0 {
                return Err(CliError::not_found("No active sessions to attach to."));
            }
            if total == 1 {
                let target = focusable_targets
                    .first()
                    .cloned()
                    .ok_or_else(|| CliError::not_found("No active sessions to attach to."))?;
                if should_emit_human_guidance(out, quiet) {
                    eprintln!(
                        "Auto-attaching to only {}: {}",
                        target.kind.label(),
                        target.id
                    );
                }
                target
            } else {
                if should_emit_human_guidance(out, quiet) {
                    eprintln!("Multiple active sessions — specify a target:\n");
                    if !ssh_items.is_empty() {
                        eprintln!("  SSH Sessions:");
                        out.print_sessions(&sessions);
                    }
                    if !local_items.is_empty() {
                        eprintln!("\n  Local Terminals:");
                        out.print_local_terminals(&locals);
                    }
                    eprintln!("\nUsage: oxt attach <SESSION-ID-OR-NAME>");
                    eprintln!("       oxt attach --latest");
                }
                return Err(CliError::usage(
                    "Multiple active sessions — specify a target.",
                ));
            }
        }
    };

    if latest && should_emit_human_guidance(out, quiet) {
        eprintln!(
            "Attaching to latest {}: {}",
            resolved_target.kind.label(),
            resolved_target.label
        );
    }

    let session_id = resolved_target.id;

    // Call the attach RPC
    let resp = conn.call(
        "attach",
        serde_json::json!({
            "session_id": session_id,
            "readonly": readonly,
        }),
    )?;

    let ws_url = resp
        .get("ws_url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| CliError::runtime("Missing ws_url in response"))?;
    let ws_token = resp
        .get("ws_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| CliError::runtime("Missing ws_token in response"))?;
    let terminal_type = resp
        .get("terminal_type")
        .and_then(|v| v.as_str())
        .unwrap_or("ssh");
    let readonly = resp
        .get("readonly")
        .and_then(|v| v.as_bool())
        .unwrap_or(readonly);
    let cols = resp.get("cols").and_then(|v| v.as_u64()).unwrap_or(80) as u16;
    let rows = resp.get("rows").and_then(|v| v.as_u64()).unwrap_or(24) as u16;

    if should_emit_human_guidance(out, quiet) {
        let mode_label = if readonly { "read-only " } else { "" };
        eprintln!(
            "Attaching {}to {} session {} ({}x{}) — type ~? for help, ~. to detach",
            mode_label, terminal_type, session_id, cols, rows
        );
    }

    // Connect WebSocket. Use a short read timeout so ws.read() returns
    // WouldBlock quickly when no data is available, keeping the event
    // loop responsive.
    let host_port = ws_url
        .strip_prefix("ws://")
        .ok_or("Invalid ws_url scheme")?;
    let tcp_stream = std::net::TcpStream::connect(host_port)
        .map_err(|e| format!("TCP connection failed: {e}"))?;
    // Handshake needs enough time; set timeout after handshake.
    let (mut ws, _response) = tungstenite::client(format!("ws://{host_port}/"), &tcp_stream)
        .map_err(|e| format!("WebSocket handshake failed: {e}"))?;

    // Authenticate: send token as first text message
    ws.send(tungstenite::Message::Text(ws_token.to_string()))
        .map_err(|e| format!("WebSocket auth failed: {e}"))?;

    // Now set a short read timeout for the event loop
    tcp_stream
        .set_read_timeout(Some(std::time::Duration::from_millis(10)))
        .map_err(|e| format!("Failed to set read timeout: {e}"))?;

    // ── Terminal size synchronization ──
    // Mirror mode: the session keeps its current size (owned by GUI).
    // We try to resize the CLI terminal to match, not the other way around.
    let (cli_cols, cli_rows) = terminal::get_terminal_size();
    if cli_cols != cols || cli_rows != rows {
        let (new_cols, new_rows) = terminal::try_resize_terminal(cols, rows);
        if new_cols != cols || new_rows != rows {
            eprintln!(
                "\x1b[33mWarning: size mismatch \u{2014} session is {}x{}, \
                 CLI terminal is {}x{}.\r\n\
                 TUI apps may render incorrectly. \
                 Resize your terminal to {}x{} for best results.\x1b[0m",
                cols, rows, new_cols, new_rows, cols, rows
            );
        }
    }

    // Enter raw mode
    let _raw_guard = terminal::RawModeGuard::enter()?;

    // Disable mouse tracking on the CLI terminal so mirrored TUI apps
    // that enable it (yazi, etc.) don't cause mouse event feedback loops.
    terminal::disable_mouse_tracking();

    // Single-threaded event loop using poll() on Unix.
    // This avoids the Arc<Mutex<WebSocket>> contention that starved
    // the writer thread in the previous two-thread design.
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::io::AsRawFd;

        let stdin_fd = libc::STDIN_FILENO;
        let ws_fd = tcp_stream.as_raw_fd();

        // Set up SIGWINCH pipe for resize notifications
        let mut sigwinch_fds = [0i32; 2];
        if unsafe { libc::pipe(sigwinch_fds.as_mut_ptr()) } != 0 {
            return Err(CliError::runtime("Failed to create SIGWINCH pipe"));
        }
        let sigwinch_read_fd = sigwinch_fds[0];
        let sigwinch_write_fd = sigwinch_fds[1];
        unsafe {
            let flags = libc::fcntl(sigwinch_read_fd, libc::F_GETFL);
            libc::fcntl(sigwinch_read_fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
        }
        terminal::install_sigwinch_handler(sigwinch_write_fd);

        let mut escape = escape::EscapeDetector::new();
        let mut stdout = std::io::stdout();
        let mut last_activity = std::time::Instant::now();
        const IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(330);

        'event_loop: loop {
            // Check idle timeout
            if last_activity.elapsed() > IDLE_TIMEOUT {
                eprintln!("\r\nConnection timed out (no data received).\r\n");
                break;
            }

            let mut pollfds = [
                libc::pollfd {
                    fd: stdin_fd,
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: sigwinch_read_fd,
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: ws_fd,
                    events: libc::POLLIN,
                    revents: 0,
                },
            ];

            let ret = unsafe { libc::poll(pollfds.as_mut_ptr(), 3, 100) };
            if ret < 0 {
                continue; // EINTR from signals
            }

            // ── WebSocket readable ──
            if pollfds[2].revents & (libc::POLLIN | libc::POLLHUP) != 0 {
                // Read as many complete messages as available
                loop {
                    match ws.read() {
                        Ok(tungstenite::Message::Binary(data)) => {
                            last_activity = std::time::Instant::now();
                            let mut cursor = std::io::Cursor::new(&data);
                            while cursor.position() < data.len() as u64 {
                                match wire::decode_frame(&mut cursor) {
                                    Ok(frame) => {
                                        if let Some(payload) = wire::frame_data(&frame) {
                                            let _ = stdout.write_all(payload);
                                            let _ = stdout.flush();
                                        } else if wire::is_heartbeat(&frame) {
                                            let mut buf = Vec::new();
                                            wire::encode_heartbeat(&mut buf);
                                            let _ = ws.send(tungstenite::Message::Binary(buf));
                                        } else if wire::is_error(&frame) {
                                            let msg = String::from_utf8_lossy(&frame.payload);
                                            eprintln!("\r\nServer error: {msg}\r\n");
                                            break 'event_loop;
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!("\r\nFrame decode error: {e}\r\n");
                                        break 'event_loop;
                                    }
                                }
                            }
                        }
                        Ok(tungstenite::Message::Close(_)) => break 'event_loop,
                        Err(tungstenite::Error::ConnectionClosed) => break 'event_loop,
                        Err(tungstenite::Error::Io(ref e))
                            if e.kind() == std::io::ErrorKind::WouldBlock =>
                        {
                            break; // no more data right now
                        }
                        Err(_) => break 'event_loop,
                        _ => {} // ignore text, ping, pong
                    }
                }
            }

            // ── SIGWINCH ──
            // Mirror mode: don't resize the server session.
            // Warn the user if CLI size doesn't match.
            if pollfds[1].revents & libc::POLLIN != 0 {
                let mut drain = [0u8; 64];
                unsafe {
                    libc::read(
                        sigwinch_read_fd,
                        drain.as_mut_ptr() as *mut libc::c_void,
                        drain.len(),
                    );
                }
                let (c, r) = terminal::get_terminal_size();
                if c != cols || r != rows {
                    let _ = stdout.write_all(
                        format!(
                            "\x1b[s\x1b[{};1H\x1b[33m\
                             [size mismatch: session {}x{}, terminal {}x{}]\
                             \x1b[0m\x1b[K\x1b[u",
                            r, cols, rows, c, r
                        )
                        .as_bytes(),
                    );
                    let _ = stdout.flush();
                }
            }

            // ── stdin ──
            if pollfds[0].revents & libc::POLLIN != 0 {
                // Drain all immediately-available data so that large pastes
                // (including bracket-paste markers from the outer terminal)
                // are forwarded as a single wire frame.
                const MAX_PASTE: usize = 256 * 1024;
                let mut raw_input = Vec::new();
                loop {
                    let mut buf = [0u8; 4096];
                    let n = unsafe {
                        libc::read(stdin_fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len())
                    };
                    if n <= 0 {
                        if raw_input.is_empty() {
                            break 'event_loop;
                        }
                        break;
                    }
                    raw_input.extend_from_slice(&buf[..n as usize]);
                    if raw_input.len() >= MAX_PASTE {
                        break;
                    }
                    // Poll with timeout 0 to check for more buffered data.
                    let mut peek = libc::pollfd {
                        fd: stdin_fd,
                        events: libc::POLLIN,
                        revents: 0,
                    };
                    if unsafe { libc::poll(&mut peek, 1, 0) } <= 0
                        || peek.revents & libc::POLLIN == 0
                    {
                        break;
                    }
                }

                let mut to_send = Vec::new();
                for &byte in &raw_input {
                    match escape.feed(byte) {
                        escape::EscapeAction::Forward(b) => to_send.push(b),
                        escape::EscapeAction::ForwardTwo(a, b) => {
                            to_send.push(a);
                            to_send.push(b);
                        }
                        escape::EscapeAction::Detach => {
                            eprintln!("\r\nDetached from session.");
                            let _ = ws.close(None);
                            // Clean up before return
                            terminal::reset_sigwinch_handler();
                            terminal::disable_mouse_tracking();
                            unsafe {
                                libc::close(sigwinch_read_fd);
                                libc::close(sigwinch_write_fd);
                            }
                            return Ok(());
                        }
                        escape::EscapeAction::ShowHelp => {
                            let _ =
                                stdout.write_all(escape::EscapeDetector::help_text().as_bytes());
                            let _ = stdout.flush();
                        }
                        escape::EscapeAction::Consumed => {}
                    }
                }

                if !readonly && !to_send.is_empty() {
                    let mut frame = Vec::new();
                    wire::encode_data(&to_send, &mut frame);
                    if ws.send(tungstenite::Message::Binary(frame)).is_err() {
                        break;
                    }
                }
            }

            // ── Connection error on WS socket ──
            if pollfds[2].revents & libc::POLLERR != 0 {
                break;
            }
        }

        // Clean up SIGWINCH handler and pipe fds
        terminal::reset_sigwinch_handler();
        terminal::disable_mouse_tracking();
        unsafe {
            libc::close(sigwinch_read_fd);
            libc::close(sigwinch_write_fd);
        }
    }

    #[cfg(windows)]
    {
        use std::io::Write;
        use std::sync::mpsc::{self, RecvTimeoutError};

        enum WindowsStdinEvent {
            Bytes(Vec<u8>),
            Closed,
            Error(String),
        }

        let (stdin_tx, stdin_rx) = mpsc::channel();
        std::thread::spawn(move || {
            use std::io::Read;

            let mut stdin = std::io::stdin();
            loop {
                let mut buf = [0u8; 4096];
                match stdin.read(&mut buf) {
                    Ok(0) => {
                        let _ = stdin_tx.send(WindowsStdinEvent::Closed);
                        break;
                    }
                    Ok(n) => {
                        if stdin_tx
                            .send(WindowsStdinEvent::Bytes(buf[..n].to_vec()))
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = stdin_tx.send(WindowsStdinEvent::Error(format!(
                            "Failed to read stdin: {e}"
                        )));
                        break;
                    }
                }
            }
        });

        let mut escape = escape::EscapeDetector::new();
        let mut stdout = std::io::stdout();
        let mut last_activity = std::time::Instant::now();
        const IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(330);

        'windows_event_loop: loop {
            if last_activity.elapsed() > IDLE_TIMEOUT {
                eprintln!("\r\nConnection timed out (no data received).\r\n");
                break;
            }

            // Try reading from WebSocket (non-blocking due to read timeout)
            match ws.read() {
                Ok(tungstenite::Message::Binary(data)) => {
                    last_activity = std::time::Instant::now();
                    let mut cursor = std::io::Cursor::new(&data);
                    while cursor.position() < data.len() as u64 {
                        if let Ok(frame) = wire::decode_frame(&mut cursor) {
                            if let Some(payload) = wire::frame_data(&frame) {
                                let _ = stdout.write_all(payload);
                                let _ = stdout.flush();
                            } else if wire::is_heartbeat(&frame) {
                                let mut buf = Vec::new();
                                wire::encode_heartbeat(&mut buf);
                                let _ = ws.send(tungstenite::Message::Binary(buf));
                            } else if wire::is_error(&frame) {
                                let msg = String::from_utf8_lossy(&frame.payload);
                                eprintln!("\r\nServer error: {msg}\r\n");
                                break 'windows_event_loop;
                            }
                        }
                    }
                    continue; // prioritize draining WS
                }
                Ok(tungstenite::Message::Close(_)) | Err(tungstenite::Error::ConnectionClosed) => {
                    break
                }
                Err(tungstenite::Error::Io(ref e))
                    if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(_) => break,
                _ => {}
            }

            match stdin_rx.recv_timeout(std::time::Duration::from_millis(10)) {
                Ok(WindowsStdinEvent::Closed) => break,
                Ok(WindowsStdinEvent::Error(err)) => return Err(err.into()),
                Ok(WindowsStdinEvent::Bytes(buf)) => {
                    // Keep a short idle window open so Windows paste bursts that
                    // arrive across multiple stdin reads still ship as one frame.
                    const MAX_PASTE: usize = 256 * 1024;
                    const COALESCE_IDLE_WINDOW: std::time::Duration =
                        std::time::Duration::from_millis(5);
                    let (raw_input, stdin_closed) = coalesce_stdin_chunks(
                        buf,
                        MAX_PASTE,
                        COALESCE_IDLE_WINDOW,
                        |idle_window| match stdin_rx.recv_timeout(idle_window) {
                            Ok(WindowsStdinEvent::Bytes(more)) => {
                                Ok(Some(StdinCoalesceEvent::Bytes(more)))
                            }
                            Ok(WindowsStdinEvent::Closed) => Ok(Some(StdinCoalesceEvent::Closed)),
                            Ok(WindowsStdinEvent::Error(err)) => Err(err),
                            Err(RecvTimeoutError::Timeout) => Ok(None),
                            Err(RecvTimeoutError::Disconnected) => {
                                Ok(Some(StdinCoalesceEvent::Closed))
                            }
                        },
                    )?;

                    let mut to_send = Vec::new();
                    for byte in raw_input {
                        match escape.feed(byte) {
                            escape::EscapeAction::Forward(b) => to_send.push(b),
                            escape::EscapeAction::ForwardTwo(a, b) => {
                                to_send.push(a);
                                to_send.push(b);
                            }
                            escape::EscapeAction::Detach => {
                                eprintln!("\r\nDetached from session.");
                                let _ = ws.close(None);
                                return Ok(());
                            }
                            escape::EscapeAction::ShowHelp => {
                                let _ = stdout
                                    .write_all(escape::EscapeDetector::help_text().as_bytes());
                                let _ = stdout.flush();
                            }
                            escape::EscapeAction::Consumed => {}
                        }
                    }
                    if !readonly && !to_send.is_empty() {
                        let mut frame = Vec::new();
                        wire::encode_data(&to_send, &mut frame);
                        if ws.send(tungstenite::Message::Binary(frame)).is_err() {
                            break;
                        }
                    }
                    if stdin_closed {
                        break;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
    }

    eprintln!("\r\nConnection closed.");
    terminal::disable_mouse_tracking();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        build_compatibility_check, build_endpoint_ownership_check, build_path_check,
        check_compatibility, classify_error_message, coalesce_stdin_chunks,
        compatibility_notice_text, protocol_ranges_overlap, read_stdin_from_reader,
        resolve_focusable_target_from_targets, select_latest_focusable_target,
        select_session_for_inspect, should_emit_human_guidance, should_render_ai_markdown, Cli,
        Commands, DoctorStatus, ExecShell, ExitCode, FocusableKind, FocusableTarget,
        StdinCoalesceEvent,
    };
    use crate::{connect, output::OutputMode};
    use clap::{CommandFactory, Parser};
    use std::io::Cursor;
    use std::sync::{mpsc, Mutex};

    static PATH_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn render_help(path: &[&str]) -> String {
        let mut command = Cli::command().term_width(100);
        let mut current = &mut command;

        for segment in path {
            current = current
                .find_subcommand_mut(segment)
                .unwrap_or_else(|| panic!("missing subcommand: {segment}"));
        }

        let mut output = Vec::new();
        current.write_long_help(&mut output).unwrap();
        String::from_utf8(output).unwrap()
    }

    fn normalize_help(text: &str) -> String {
        text.replace("\r\n", "\n")
            .trim_end_matches('\n')
            .to_string()
    }

    #[test]
    fn main_help_matches_snapshot() {
        assert_eq!(
            normalize_help(&render_help(&[])),
            normalize_help(include_str!("../tests/snapshots/oxt-help.txt"))
        );
    }

    #[test]
    fn list_help_matches_snapshot() {
        assert_eq!(
            normalize_help(&render_help(&["list"])),
            normalize_help(include_str!("../tests/snapshots/oxt-list-help.txt"))
        );
    }

    #[test]
    fn forward_help_matches_snapshot() {
        assert_eq!(
            normalize_help(&render_help(&["forward"])),
            normalize_help(include_str!("../tests/snapshots/oxt-forward-help.txt"))
        );
    }

    #[test]
    fn ask_help_matches_snapshot() {
        assert_eq!(
            normalize_help(&render_help(&["ask"])),
            normalize_help(include_str!("../tests/snapshots/oxt-ask-help.txt"))
        );
    }

    #[test]
    fn exec_help_matches_snapshot() {
        assert_eq!(
            normalize_help(&render_help(&["exec"])),
            normalize_help(include_str!("../tests/snapshots/oxt-exec-help.txt"))
        );
    }

    #[test]
    fn doctor_help_matches_snapshot() {
        assert_eq!(
            normalize_help(&render_help(&["doctor"])),
            normalize_help(include_str!("../tests/snapshots/oxt-doctor-help.txt"))
        );
    }

    #[test]
    fn status_help_matches_snapshot() {
        assert_eq!(
            normalize_help(&render_help(&["status"])),
            normalize_help(include_str!("../tests/snapshots/oxt-status-help.txt"))
        );
    }

    #[test]
    fn health_help_matches_snapshot() {
        assert_eq!(
            normalize_help(&render_help(&["health"])),
            normalize_help(include_str!("../tests/snapshots/oxt-health-help.txt"))
        );
    }

    #[test]
    fn connect_help_matches_snapshot() {
        assert_eq!(
            normalize_help(&render_help(&["connect"])),
            normalize_help(include_str!("../tests/snapshots/oxt-connect-help.txt"))
        );
    }

    #[test]
    fn focus_help_matches_snapshot() {
        assert_eq!(
            normalize_help(&render_help(&["focus"])),
            normalize_help(include_str!("../tests/snapshots/oxt-focus-help.txt"))
        );
    }

    #[test]
    fn attach_help_matches_snapshot() {
        assert_eq!(
            normalize_help(&render_help(&["attach"])),
            normalize_help(include_str!("../tests/snapshots/oxt-attach-help.txt"))
        );
    }

    #[test]
    fn session_help_matches_snapshot() {
        assert_eq!(
            normalize_help(&render_help(&["session"])),
            normalize_help(include_str!("../tests/snapshots/oxt-session-help.txt"))
        );
    }

    #[test]
    fn stdin_reader_accepts_exact_limit() {
        let input = vec![b'a'; 8];
        let mut reader = Cursor::new(input);

        let result = read_stdin_from_reader(&mut reader, 8).unwrap();

        assert_eq!(result.len(), 8);
    }

    #[test]
    fn stdin_reader_rejects_oversized_input() {
        let input = vec![b'a'; 9];
        let mut reader = Cursor::new(input);

        let err = read_stdin_from_reader(&mut reader, 8).unwrap_err();

        assert!(err.contains("exceeds"));
    }

    #[test]
    fn stdin_chunk_coalescing_waits_for_delayed_chunk() {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(1));
            tx.send(StdinCoalesceEvent::Bytes(vec![b'b'; 4])).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(20));
        });

        let (raw_input, stdin_closed) = coalesce_stdin_chunks(
            vec![b'a'; 4],
            256,
            std::time::Duration::from_millis(10),
            |idle_window| match rx.recv_timeout(idle_window) {
                Ok(event) => Ok::<Option<StdinCoalesceEvent>, String>(Some(event)),
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    Ok::<Option<StdinCoalesceEvent>, String>(None)
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    Ok::<Option<StdinCoalesceEvent>, String>(Some(StdinCoalesceEvent::Closed))
                }
            },
        )
        .unwrap();

        assert_eq!(raw_input, b"aaaabbbb");
        assert!(!stdin_closed);
    }

    #[test]
    fn stdin_chunk_coalescing_preserves_closed_signal_after_buffered_bytes() {
        let (tx, rx) = mpsc::channel();
        tx.send(StdinCoalesceEvent::Bytes(vec![b'b'; 4])).unwrap();
        tx.send(StdinCoalesceEvent::Closed).unwrap();
        drop(tx);

        let (raw_input, stdin_closed) = coalesce_stdin_chunks(
            vec![b'a'; 4],
            256,
            std::time::Duration::from_millis(10),
            |idle_window| match rx.recv_timeout(idle_window) {
                Ok(event) => Ok::<Option<StdinCoalesceEvent>, String>(Some(event)),
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    Ok::<Option<StdinCoalesceEvent>, String>(None)
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    Ok::<Option<StdinCoalesceEvent>, String>(Some(StdinCoalesceEvent::Closed))
                }
            },
        )
        .unwrap();

        assert_eq!(raw_input, b"aaaabbbb");
        assert!(stdin_closed);
    }

    #[test]
    fn legacy_status_without_cli_api_is_rejected() {
        let report = check_compatibility(serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"),
            "sessions": 0,
            "connections": { "ssh": 0, "local": 0 }
        }));

        assert!(report
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("does not expose CLI API"));
        assert!(report.warning.is_none());
    }

    #[test]
    fn protocol_range_overlap_accepts_shared_version() {
        assert!(protocol_ranges_overlap(1, 2, 2, 3));
        assert!(protocol_ranges_overlap(2, 2, 1, 2));
        assert!(!protocol_ranges_overlap(1, 1, 2, 2));
    }

    #[test]
    fn compatibility_warns_on_app_version_mismatch() {
        let report = check_compatibility(serde_json::json!({
            "version": "9.9.9",
            "cli_api": { "version": 2, "min_supported": 1 },
            "sessions": 0,
            "connections": { "ssh": 0, "local": 0 }
        }));

        assert!(report.error.is_none());
        assert!(report
            .warning
            .as_deref()
            .unwrap_or_default()
            .contains("9.9.9"));
    }

    #[test]
    fn compatibility_rejects_non_overlapping_protocol_ranges() {
        let report = check_compatibility(serde_json::json!({
            "version": "9.9.9",
            "cli_api": { "version": 4, "min_supported": 4 },
            "sessions": 0,
            "connections": { "ssh": 0, "local": 0 }
        }));

        assert!(report.warning.is_none());
        assert!(report
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("CLI API mismatch"));
    }

    #[test]
    fn compatibility_check_builds_ok_doctor_item() {
        let check = build_compatibility_check(&serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"),
            "cli_api": { "version": 2, "min_supported": 1 },
            "sessions": 0,
            "connections": { "ssh": 0, "local": 0 }
        }));

        assert_eq!(check.status, DoctorStatus::Ok);
        assert!(check.summary.contains("CLI API ranges overlap"));
    }

    #[test]
    fn compatibility_rejects_pre_phase5_server_contract() {
        let report = check_compatibility(serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"),
            "cli_api": { "version": 1, "min_supported": 1 },
            "sessions": 0,
            "connections": { "ssh": 0, "local": 0 }
        }));

        assert!(report.warning.is_none());
        assert!(report
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("CLI API mismatch"));
    }

    #[cfg(unix)]
    #[test]
    fn path_lookup_warns_on_non_executable_file() {
        use std::os::unix::fs::PermissionsExt;

        let _guard = PATH_TEST_LOCK.lock().unwrap();
        let temp_dir = tempfile::TempDir::new().unwrap();
        let path_dir = temp_dir.path().join("path-dir");
        let current_dir = temp_dir.path().join("current-dir");
        std::fs::create_dir_all(&path_dir).unwrap();
        std::fs::create_dir_all(&current_dir).unwrap();

        let path_hit = path_dir.join("oxt");
        let current_exe = current_dir.join("oxt");
        std::fs::write(&path_hit, b"#!/bin/sh\n").unwrap();
        std::fs::write(&current_exe, b"#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&path_hit, std::fs::Permissions::from_mode(0o644)).unwrap();
        std::fs::set_permissions(&current_exe, std::fs::Permissions::from_mode(0o755)).unwrap();

        let old_path = std::env::var_os("PATH");
        std::env::set_var(
            "PATH",
            std::env::join_paths([path_dir.as_path(), current_dir.as_path()]).unwrap(),
        );

        let check = build_path_check(Some(&current_exe));

        match old_path {
            Some(old_path) => std::env::set_var("PATH", old_path),
            None => std::env::remove_var("PATH"),
        }

        assert_eq!(check.status, DoctorStatus::Warn);
        assert!(check.summary.contains("non-executable file"));
    }

    #[cfg(unix)]
    #[test]
    fn path_lookup_warns_on_broken_symlink() {
        let _guard = PATH_TEST_LOCK.lock().unwrap();
        let temp_dir = tempfile::TempDir::new().unwrap();
        let path_dir = temp_dir.path().join("path-dir");
        let current_dir = temp_dir.path().join("current-dir");
        std::fs::create_dir_all(&path_dir).unwrap();
        std::fs::create_dir_all(&current_dir).unwrap();

        let broken_target = path_dir.join("missing-oxt");
        let path_hit = path_dir.join("oxt");
        let current_exe = current_dir.join("oxt");
        std::fs::write(&current_exe, b"#!/bin/sh\n").unwrap();
        std::os::unix::fs::symlink(&broken_target, &path_hit).unwrap();

        let old_path = std::env::var_os("PATH");
        std::env::set_var(
            "PATH",
            std::env::join_paths([path_dir.as_path(), current_dir.as_path()]).unwrap(),
        );

        let check = build_path_check(Some(&current_exe));

        match old_path {
            Some(old_path) => std::env::set_var("PATH", old_path),
            None => std::env::remove_var("PATH"),
        }

        assert_eq!(check.status, DoctorStatus::Warn);
        assert!(check.summary.contains("broken symlink"));
    }

    #[test]
    fn compatibility_notice_text_respects_json_and_quiet() {
        assert_eq!(
            compatibility_notice_text(&OutputMode::Human, false, Some("version mismatch")),
            Some("version mismatch".to_string())
        );
        assert_eq!(
            compatibility_notice_text(&OutputMode::Human, true, Some("version mismatch")),
            None
        );
        assert_eq!(
            compatibility_notice_text(&OutputMode::Json, false, Some("version mismatch")),
            None
        );
    }

    #[test]
    fn should_emit_human_guidance_respects_json_and_quiet() {
        assert!(should_emit_human_guidance(&OutputMode::Human, false));
        assert!(!should_emit_human_guidance(&OutputMode::Human, true));
        assert!(!should_emit_human_guidance(&OutputMode::Json, false));
    }

    #[test]
    fn ask_render_mode_respects_raw_and_json_contract() {
        assert!(should_render_ai_markdown(false, &OutputMode::Human));
        assert!(!should_render_ai_markdown(true, &OutputMode::Human));
        assert!(!should_render_ai_markdown(false, &OutputMode::Json));
    }

    #[test]
    fn exec_shell_accepts_powershell_aliases() {
        for value in ["powershell", "power-shell"] {
            let cli = Cli::try_parse_from(["oxt", "exec", "--shell", value, "echo hi"])
                .expect("exec shell should parse");

            match cli.command {
                Commands::Exec { shell, .. } => assert_eq!(shell, Some(ExecShell::PowerShell)),
                _ => panic!("expected exec command"),
            }
        }
    }

    #[test]
    fn classify_error_message_maps_timeout_and_not_found() {
        let timeout = classify_error_message("Timed out waiting for session to appear".to_string());
        assert_eq!(timeout.code, "timeout");
        assert_eq!(timeout.exit_code, ExitCode::Timeout);

        let not_found = classify_error_message("Connection not found: prod".to_string());
        assert_eq!(not_found.code, "not_found");
        assert_eq!(not_found.exit_code, ExitCode::NotFound);
    }

    #[test]
    fn select_session_for_inspect_rejects_ambiguous_name_matches() {
        let sessions = vec![
            serde_json::json!({ "id": "session-1", "name": "prod" }),
            serde_json::json!({ "id": "session-2", "name": "prod" }),
        ];

        let error = select_session_for_inspect(&sessions, "prod").unwrap_err();

        assert_eq!(error.code, "usage_error");
        assert_eq!(error.exit_code, ExitCode::Usage);
    }

    #[test]
    fn select_session_for_inspect_prefers_exact_id_before_prefix() {
        let sessions = vec![
            serde_json::json!({ "id": "abc123", "name": "one" }),
            serde_json::json!({ "id": "abc1234", "name": "two" }),
        ];

        let selected = select_session_for_inspect(&sessions, "abc123").unwrap();

        assert_eq!(selected["id"], "abc123");
    }

    #[test]
    fn resolve_focusable_target_rejects_ambiguous_exact_labels() {
        let targets = vec![
            FocusableTarget {
                id: "ssh-1".to_string(),
                label: "prod".to_string(),
                shell_id: None,
                created_at: Some("2026-04-19T12:00:00Z".to_string()),
                kind: FocusableKind::Ssh,
            },
            FocusableTarget {
                id: "local-1".to_string(),
                label: "prod".to_string(),
                shell_id: Some("shell-prod".to_string()),
                created_at: Some("2026-04-19T12:00:01Z".to_string()),
                kind: FocusableKind::Local,
            },
        ];

        let error = resolve_focusable_target_from_targets(&targets, "prod").unwrap_err();

        assert_eq!(error.code, "usage_error");
        assert_eq!(error.exit_code, ExitCode::Usage);
    }

    #[test]
    fn resolve_focusable_target_prefers_exact_id_before_label() {
        let targets = vec![
            FocusableTarget {
                id: "abc123".to_string(),
                label: "shell-a".to_string(),
                shell_id: Some("shell-a".to_string()),
                created_at: Some("2026-04-19T12:00:00Z".to_string()),
                kind: FocusableKind::Local,
            },
            FocusableTarget {
                id: "shell-a".to_string(),
                label: "other".to_string(),
                shell_id: None,
                created_at: Some("2026-04-19T12:00:01Z".to_string()),
                kind: FocusableKind::Ssh,
            },
        ];

        let selected = resolve_focusable_target_from_targets(&targets, "shell-a").unwrap();

        assert_eq!(selected.id, "shell-a");
        assert_eq!(selected.kind, FocusableKind::Ssh);
    }

    #[test]
    fn select_latest_focusable_target_prefers_newer_created_at() {
        let targets = vec![
            FocusableTarget {
                id: "ssh-1".to_string(),
                label: "prod".to_string(),
                shell_id: None,
                created_at: Some("2026-04-19T12:00:00Z".to_string()),
                kind: FocusableKind::Ssh,
            },
            FocusableTarget {
                id: "local-1".to_string(),
                label: "zsh".to_string(),
                shell_id: Some("shell-zsh".to_string()),
                created_at: Some("2026-04-19T12:00:05Z".to_string()),
                kind: FocusableKind::Local,
            },
        ];

        let selected = select_latest_focusable_target(&targets).unwrap();

        assert_eq!(selected.id, "local-1");
        assert_eq!(selected.kind, FocusableKind::Local);
    }

    #[test]
    fn ownership_check_skips_non_socket_endpoints() {
        let check = build_endpoint_ownership_check(&connect::EndpointInspection {
            exists: true,
            kind: Some("file".to_string()),
            metadata_error: None,
            ownership_matches: Some(true),
            owner_uid: Some(501),
            current_uid: Some(501),
        });

        assert_eq!(check.status, DoctorStatus::Warn);
        assert!(check.summary.contains("not a Unix socket"));
    }
}
