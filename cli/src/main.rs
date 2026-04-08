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
mod terminal;
mod wire;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;

const CLI_API_CURRENT_VERSION: u64 = 1;
const CLI_API_MIN_SUPPORTED_VERSION: u64 = 1;

struct CompatibilityReport {
    status: serde_json::Value,
    warning: Option<String>,
    error: Option<String>,
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
    Status,

    /// List resources
    List {
        #[command(subcommand)]
        what: ListTarget,
    },

    /// Show connection health
    Health {
        /// Session ID (omit to show all)
        session_id: Option<String>,
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
    },

    /// Connect to a saved connection (opens in GUI)
    Connect {
        /// Connection name, host, or ID
        target: String,
    },

    /// Open a new local terminal tab
    Open {
        /// Working directory (default: current directory)
        path: Option<String>,
    },

    /// Focus an existing session tab
    Focus {
        /// Session ID or name (omit to list available sessions)
        target: Option<String>,
    },

    /// Attach to a running session (mirror terminal I/O)
    Attach {
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
    /// List active port forwards
    Forwards {
        /// Session ID (omit to show all sessions)
        session_id: Option<String>,
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

    if let Err(e) = result {
        if out.is_json() {
            let err = serde_json::json!({ "error": e });
            eprintln!("{}", serde_json::to_string(&err).unwrap_or_default());
        } else {
            eprintln!("error: {e}");
        }
        std::process::exit(1);
    }
}

fn run(cli: &Cli, out: &output::OutputMode) -> Result<(), String> {
    // Commands that don't need IPC
    match &cli.command {
        Commands::Version => {
            out.print_version();
            return Ok(());
        }
        Commands::Completions { shell } => {
            clap_complete::generate(*shell, &mut Cli::command(), "oxt", &mut std::io::stdout());
            return Ok(());
        }
        _ => {}
    }

    let mut conn = connect::IpcConnection::connect(cli.socket.as_deref(), cli.timeout)?;
    let compatibility = fetch_compatibility_report(&mut conn)?;

    match &cli.command {
        Commands::Status => {
            out.print_status(&compatibility.status);
            emit_compatibility_notice(
                out,
                compatibility
                    .error
                    .as_deref()
                    .or(compatibility.warning.as_deref()),
            );
        }
        Commands::List { what } => match what {
            _ if compatibility.error.is_some() => {
                return Err(compatibility.error.unwrap_or_default())
            }
            ListTarget::Connections => {
                emit_compatibility_notice(out, compatibility.warning.as_deref());
                let resp = conn.call("list_saved_connections", serde_json::json!({}))?;
                out.print_connections(&resp);
            }
            ListTarget::Sessions => {
                emit_compatibility_notice(out, compatibility.warning.as_deref());
                let resp = conn.call("list_sessions", serde_json::json!({}))?;
                out.print_sessions(&resp);
            }
            ListTarget::Forwards { session_id } => {
                emit_compatibility_notice(out, compatibility.warning.as_deref());
                let params = match session_id {
                    Some(id) => serde_json::json!({ "session_id": id }),
                    None => serde_json::json!({}),
                };
                let resp = conn.call("list_forwards", params)?;
                out.print_forwards(&resp);
            }
        },
        Commands::Health { session_id } => {
            return_if_incompatible(&compatibility)?;
            emit_compatibility_notice(out, compatibility.warning.as_deref());
            let params = match session_id {
                Some(id) => serde_json::json!({ "session_id": id }),
                None => serde_json::json!({}),
            };
            let resp = conn.call("health", params)?;
            out.print_health(&resp, session_id.is_some());
        }
        Commands::Disconnect { target } => {
            return_if_incompatible(&compatibility)?;
            emit_compatibility_notice(out, compatibility.warning.as_deref());
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
                emit_compatibility_notice(out, compatibility.warning.as_deref());
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
                emit_compatibility_notice(out, compatibility.warning.as_deref());
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
                emit_compatibility_notice(out, compatibility.warning.as_deref());
                let resp = conn.call("config_list", serde_json::json!({}))?;
                out.print_config_list(&resp);
            }
            ConfigAction::Get { name } => {
                return_if_incompatible(&compatibility)?;
                emit_compatibility_notice(out, compatibility.warning.as_deref());
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
            emit_compatibility_notice(out, compatibility.warning.as_deref());
            let prompt_text = prompt.join(" ");
            if prompt_text.is_empty() && is_terminal_stdin() {
                return Err("No prompt provided. Usage: oxt ask \"your question\"".to_string());
            }

            // Read stdin if piped
            let context = if !is_terminal_stdin() {
                Some(read_stdin()?)
            } else {
                None
            };

            let mut params = serde_json::json!({
                "prompt": if prompt_text.is_empty() { "Analyze the following".to_string() } else { prompt_text },
                "stream": !no_stream,
            });
            if let Some(ctx) = &context {
                params["context"] = serde_json::json!(ctx);
            }
            if let Some(s) = session {
                let sid = resolve_session_id(&mut conn, s)?;
                params["session_id"] = serde_json::json!(sid);
            }
            if let Some(m) = model {
                params["model"] = serde_json::json!(m);
            }
            if let Some(p) = provider {
                params["provider"] = serde_json::json!(p);
            }
            if let Some(cid) = continue_id {
                params["conversation_id"] = serde_json::json!(cid);
            }

            // Determine markdown rendering mode:
            // --raw or piped stdout → raw text; TTY stdout → markdown render
            let use_markdown = !raw && is_terminal_stdout();

            if *no_stream {
                let resp = conn.call("ask", params)?;
                let text = resp
                    .get("text")
                    .and_then(|v| v.as_str())
                    .or_else(|| resp.get("content").and_then(|v| v.as_str()))
                    .unwrap_or("");
                if use_markdown {
                    render_markdown(text);
                } else {
                    out.print_ai_response(&resp);
                }
                print_conversation_id(&resp);
            } else {
                let mut accumulated = String::new();
                let resp = conn.call_streaming("ask", params, |text| {
                    if use_markdown {
                        accumulated.push_str(text);
                    } else {
                        use std::io::Write;
                        print!("{text}");
                        let _ = std::io::stdout().flush();
                    }
                })?;
                if use_markdown {
                    render_markdown(&accumulated);
                } else {
                    println!();
                }
                print_conversation_id(&resp);
            }
        }
        Commands::Exec {
            prompt,
            session,
            model,
            provider,
        } => {
            return_if_incompatible(&compatibility)?;
            emit_compatibility_notice(out, compatibility.warning.as_deref());
            let prompt_text = prompt.join(" ");
            if prompt_text.is_empty() && is_terminal_stdin() {
                return Err("No prompt provided. Usage: oxt exec \"generate a script\"".to_string());
            }

            let context = if !is_terminal_stdin() {
                Some(read_stdin()?)
            } else {
                None
            };

            let mut params = serde_json::json!({
                "prompt": if prompt_text.is_empty() { "Generate code for the following".to_string() } else { prompt_text },
                "stream": true,
                "exec_mode": true,
            });
            if let Some(ctx) = &context {
                params["context"] = serde_json::json!(ctx);
            }
            if let Some(s) = session {
                let sid = resolve_session_id(&mut conn, s)?;
                params["session_id"] = serde_json::json!(sid);
            }
            if let Some(m) = model {
                params["model"] = serde_json::json!(m);
            }
            if let Some(p) = provider {
                params["provider"] = serde_json::json!(p);
            }

            conn.call_streaming("ask", params, |text| {
                use std::io::Write;
                print!("{text}");
                let _ = std::io::stdout().flush();
            })?;
            println!();
        }
        Commands::Connect { target } => {
            return_if_incompatible(&compatibility)?;
            emit_compatibility_notice(out, compatibility.warning.as_deref());
            let resp = conn.call("connect", serde_json::json!({ "target": target }))?;
            out.print_connect_result(&resp);
        }
        Commands::Sftp { action } => match action {
            SftpAction::Ls { session, path } => {
                return_if_incompatible(&compatibility)?;
                emit_compatibility_notice(out, compatibility.warning.as_deref());
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
                emit_compatibility_notice(out, compatibility.warning.as_deref());
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
                emit_compatibility_notice(out, compatibility.warning.as_deref());
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
                emit_compatibility_notice(out, compatibility.warning.as_deref());
                let resp = conn.call("import_list", serde_json::json!({}))?;
                out.print_import_list(&resp);
            }
            ImportAction::Add { aliases, all } => {
                return_if_incompatible(&compatibility)?;
                emit_compatibility_notice(out, compatibility.warning.as_deref());
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
                        eprintln!("All hosts are already imported.");
                    } else {
                        return Err("No aliases specified. Usage: oxt import add <alias1> <alias2> ... or --all".to_string());
                    }
                    return Ok(());
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
            emit_compatibility_notice(out, compatibility.warning.as_deref());
            let dir = path.clone().unwrap_or_else(|| {
                std::env::current_dir()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| ".".to_string())
            });
            let resp = conn.call("open_tab", serde_json::json!({ "path": dir }))?;
            out.print_json(&resp);
        }
        Commands::Focus { target } => {
            return_if_incompatible(&compatibility)?;
            emit_compatibility_notice(out, compatibility.warning.as_deref());
            match target {
                Some(t) => {
                    let resp = conn.call("focus_tab", serde_json::json!({ "target": t }))?;
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
                            eprintln!("No active tabs to focus.");
                            std::process::exit(1);
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
                            eprintln!("Auto-focusing: {label}");
                            let resp =
                                conn.call("focus_tab", serde_json::json!({ "target": id }))?;
                            out.print_json(&resp);
                        }
                        _ => {
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
                            std::process::exit(1);
                        }
                    }
                }
            }
        }
        Commands::Ping => {
            emit_compatibility_notice(
                out,
                compatibility
                    .error
                    .as_deref()
                    .or(compatibility.warning.as_deref()),
            );
            let resp = conn.call("ping", serde_json::json!({}))?;
            out.print_json(&resp);
        }
        Commands::Attach { target } => {
            return_if_incompatible(&compatibility)?;
            emit_compatibility_notice(out, compatibility.warning.as_deref());
            return run_attach(&mut conn, out, target.as_deref());
        }
        Commands::Version | Commands::Completions { .. } => unreachable!(),
    }

    Ok(())
}

fn fetch_compatibility_report(
    conn: &mut connect::IpcConnection,
) -> Result<CompatibilityReport, String> {
    let status = conn.call("status", serde_json::json!({}))?;
    Ok(check_compatibility(status))
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

fn return_if_incompatible(report: &CompatibilityReport) -> Result<(), String> {
    if let Some(error) = &report.error {
        return Err(error.clone());
    }
    Ok(())
}

fn emit_compatibility_notice(out: &output::OutputMode, message: Option<&str>) {
    if out.is_json() {
        return;
    }
    if let Some(message) = message {
        eprintln!("warning: {message}");
    }
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

fn is_terminal_stdout() -> bool {
    use std::io::IsTerminal;
    std::io::stdout().is_terminal()
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

/// Render markdown text to the terminal using termimad.
fn render_markdown(text: &str) {
    let skin = termimad::MadSkin::default();
    // termimad writes ANSI-colored markdown to the terminal
    skin.print_text(text);
}

/// Print the conversation ID from the response for `--continue` use.
fn print_conversation_id(resp: &serde_json::Value) {
    if let Some(cid) = resp.get("conversation_id").and_then(|v| v.as_str()) {
        eprintln!("\n\x1b[2mConversation: {cid}\x1b[0m");
        eprintln!("\x1b[2mContinue with: oxt ask --continue {cid} \"your follow-up\"\x1b[0m");
    }
}

/// Resolve a target to a session ID, checking both SSH and local sessions.
fn resolve_any_session_id(
    conn: &mut connect::IpcConnection,
    target: &str,
) -> Result<String, String> {
    // Try SSH sessions first
    if let Ok(id) = resolve_session_id(conn, target) {
        return Ok(id);
    }

    // Try local terminals
    let locals = conn
        .call("list_local_terminals", serde_json::json!({}))
        .unwrap_or(serde_json::json!([]));
    if let Some(items) = locals.as_array() {
        // Exact ID match
        if let Some(s) = items
            .iter()
            .find(|s| s.get("id").and_then(|v| v.as_str()) == Some(target))
        {
            return Ok(s["id"].as_str().unwrap().to_string());
        }
        // Shell name match
        if let Some(s) = items.iter().find(|s| {
            s.get("shell_name").and_then(|v| v.as_str()) == Some(target)
                || s.get("shell_id").and_then(|v| v.as_str()) == Some(target)
        }) {
            return Ok(s["id"].as_str().unwrap().to_string());
        }
        // Partial ID match
        if let Some(s) = items.iter().find(|s| {
            s.get("id")
                .and_then(|v| v.as_str())
                .map(|id| id.starts_with(target))
                .unwrap_or(false)
        }) {
            return Ok(s["id"].as_str().unwrap().to_string());
        }
    }

    Err(format!("Session not found: {target}"))
}

/// Main entry point for `oxt attach`.
fn run_attach(
    conn: &mut connect::IpcConnection,
    out: &output::OutputMode,
    target: Option<&str>,
) -> Result<(), String> {
    // If no target, list available sessions
    let session_id = match target {
        Some(t) => resolve_any_session_id(conn, t)?,
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
                return Err("No active sessions to attach to.".to_string());
            }
            if total == 1 {
                let id = if let Some(s) = ssh_items.first() {
                    s.get("id").and_then(|v| v.as_str()).unwrap_or("")
                } else {
                    local_items[0]
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                };
                eprintln!("Auto-attaching to only session: {id}");
                id.to_string()
            } else {
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
                std::process::exit(1);
            }
        }
    };

    // Call the attach RPC
    let resp = conn.call("attach", serde_json::json!({ "session_id": session_id }))?;

    let ws_url = resp
        .get("ws_url")
        .and_then(|v| v.as_str())
        .ok_or("Missing ws_url in response")?;
    let ws_token = resp
        .get("ws_token")
        .and_then(|v| v.as_str())
        .ok_or("Missing ws_token in response")?;
    let terminal_type = resp
        .get("terminal_type")
        .and_then(|v| v.as_str())
        .unwrap_or("ssh");
    let cols = resp.get("cols").and_then(|v| v.as_u64()).unwrap_or(80) as u16;
    let rows = resp.get("rows").and_then(|v| v.as_u64()).unwrap_or(24) as u16;

    eprintln!(
        "Attaching to {} session {} ({}x{}) — type ~? for help, ~. to detach",
        terminal_type, session_id, cols, rows
    );

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
            return Err("Failed to create SIGWINCH pipe".to_string());
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
                let mut buf = [0u8; 4096];
                let n = unsafe {
                    libc::read(stdin_fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len())
                };
                if n <= 0 {
                    break;
                }

                let mut to_send = Vec::new();
                for &byte in &buf[..n as usize] {
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

                if !to_send.is_empty() {
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
                Ok(WindowsStdinEvent::Error(err)) => return Err(err),
                Ok(WindowsStdinEvent::Bytes(buf)) => {
                    let mut to_send = Vec::new();
                    for byte in buf {
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
                    if !to_send.is_empty() {
                        let mut frame = Vec::new();
                        wire::encode_data(&to_send, &mut frame);
                        if ws.send(tungstenite::Message::Binary(frame)).is_err() {
                            break;
                        }
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
    use super::{check_compatibility, protocol_ranges_overlap, read_stdin_from_reader};
    use std::io::Cursor;

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
            "cli_api": { "version": 1, "min_supported": 1 },
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
            "cli_api": { "version": 3, "min_supported": 3 },
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
}
