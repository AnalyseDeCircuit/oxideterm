// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Output formatting for CLI responses.
//!
//! Automatically detects terminal vs pipe context.
//! - Terminal: human-readable colored tables
//! - Pipe: structured JSON

use std::io::IsTerminal;

use serde_json::Value;

/// Output mode for CLI responses.
pub enum OutputMode {
    Human,
    Json,
}

impl OutputMode {
    /// Detect output mode based on terminal detection and flags.
    pub fn detect(force_json: bool) -> Self {
        if force_json || !is_terminal_stdout() {
            Self::Json
        } else {
            Self::Human
        }
    }

    pub fn is_json(&self) -> bool {
        matches!(self, Self::Json)
    }

    /// Print raw JSON value (pretty for human, compact for pipe).
    pub fn print_json(&self, value: &Value) {
        match self {
            Self::Human => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(value).unwrap_or_default()
                );
            }
            Self::Json => {
                println!("{}", serde_json::to_string(value).unwrap_or_default());
            }
        }
    }

    /// Print status response.
    pub fn print_status(&self, value: &Value) {
        match self {
            Self::Json => {
                println!("{}", serde_json::to_string(value).unwrap_or_default());
            }
            Self::Human => {
                let version = value
                    .get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let cli_api_version = value.pointer("/cli_api/version").and_then(|v| v.as_u64());
                let cli_api_min_supported = value
                    .pointer("/cli_api/min_supported")
                    .and_then(|v| v.as_u64());
                let sessions = value.get("sessions").and_then(|v| v.as_u64()).unwrap_or(0);
                let ssh = value
                    .pointer("/connections/ssh")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let local = value
                    .pointer("/connections/local")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);

                println!("OxideTerm v{version}");
                if let Some(api_version) = cli_api_version {
                    let api_min = cli_api_min_supported.unwrap_or(api_version);
                    if api_min == api_version {
                        println!("  CLI API:       v{api_version}");
                    } else {
                        println!("  CLI API:       {api_min}-{api_version}");
                    }
                }
                println!("  Sessions:      {sessions} active");
                println!("  Connections:   {ssh} SSH, {local} local");
            }
        }
    }

    /// Print saved connections list.
    pub fn print_connections(&self, value: &Value) {
        match self {
            Self::Json => {
                println!("{}", serde_json::to_string(value).unwrap_or_default());
            }
            Self::Human => {
                let items = value.as_array().map(|a| a.as_slice()).unwrap_or(&[]);
                if items.is_empty() {
                    println!("No saved connections");
                    return;
                }

                println!(
                    "  {:<16} {:<24} {:<6} {:<10} {}",
                    "NAME", "HOST", "PORT", "USER", "TYPE"
                );
                for item in items {
                    let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("-");
                    let host = item.get("host").and_then(|v| v.as_str()).unwrap_or("-");
                    let port = item.get("port").and_then(|v| v.as_u64()).unwrap_or(22);
                    let user = item.get("username").and_then(|v| v.as_str()).unwrap_or("-");
                    let auth = item
                        .get("auth_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("-");
                    println!(
                        "  {:<16} {:<24} {:<6} {:<10} {}",
                        sanitize_display(name),
                        sanitize_display(host),
                        port,
                        sanitize_display(user),
                        auth
                    );
                }
            }
        }
    }

    /// Print active sessions list.
    pub fn print_sessions(&self, value: &Value) {
        match self {
            Self::Json => {
                println!("{}", serde_json::to_string(value).unwrap_or_default());
            }
            Self::Human => {
                let items = value.as_array().map(|a| a.as_slice()).unwrap_or(&[]);
                if items.is_empty() {
                    println!("No active sessions");
                    return;
                }

                println!(
                    "  {:<14} {:<16} {:<24} {:<10} {}",
                    "ID", "NAME", "HOST", "STATE", "UPTIME"
                );
                for item in items {
                    let id = item.get("id").and_then(|v| v.as_str()).unwrap_or("-");
                    let short_id = if id.len() > 12 { &id[..12] } else { id };
                    let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("-");
                    let host = item.get("host").and_then(|v| v.as_str()).unwrap_or("-");
                    let state = item.get("state").and_then(|v| v.as_str()).unwrap_or("-");
                    let uptime = item
                        .get("uptime_secs")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let uptime_str = format_duration(uptime);
                    println!(
                        "  {:<14} {:<16} {:<24} {:<10} {}",
                        short_id,
                        sanitize_display(name),
                        sanitize_display(host),
                        state,
                        uptime_str
                    );
                }
            }
        }
    }

    /// Print local terminals list.
    pub fn print_local_terminals(&self, value: &Value) {
        match self {
            Self::Json => {
                println!("{}", serde_json::to_string(value).unwrap_or_default());
            }
            Self::Human => {
                let items = value.as_array().map(|a| a.as_slice()).unwrap_or(&[]);
                if items.is_empty() {
                    println!("  No local terminals");
                    return;
                }

                println!(
                    "  {:<14} {:<16} {:<10} {}",
                    "ID", "SHELL", "RUNNING", "DETACHED"
                );
                for item in items {
                    let id = item.get("id").and_then(|v| v.as_str()).unwrap_or("-");
                    let short_id = if id.len() > 12 { &id[..12] } else { id };
                    let shell = item
                        .get("shell_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("-");
                    let running = item
                        .get("running")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let detached = item
                        .get("detached")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    println!(
                        "  {:<14} {:<16} {:<10} {}",
                        short_id,
                        sanitize_display(shell),
                        if running { "yes" } else { "no" },
                        if detached { "yes" } else { "no" },
                    );
                }
            }
        }
    }

    /// Print port forwards list.
    pub fn print_forwards(&self, value: &Value) {
        match self {
            Self::Json => {
                println!("{}", serde_json::to_string(value).unwrap_or_default());
            }
            Self::Human => {
                let items = value.as_array().map(|a| a.as_slice()).unwrap_or(&[]);
                if items.is_empty() {
                    println!("No active port forwards");
                    return;
                }

                println!(
                    "  {:<10} {:<8} {:<24} {:<24} {:<10} {}",
                    "SESSION", "TYPE", "BIND", "TARGET", "STATUS", "DESC"
                );
                for item in items {
                    let session = item
                        .get("session_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("-");
                    let short_session = if session.len() > 8 {
                        &session[..8]
                    } else {
                        session
                    };
                    let fwd_type = item
                        .get("forward_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("-");
                    let bind_addr = item
                        .get("bind_address")
                        .and_then(|v| v.as_str())
                        .unwrap_or("0.0.0.0");
                    let bind_port = item.get("bind_port").and_then(|v| v.as_u64()).unwrap_or(0);
                    let target_host = item
                        .get("target_host")
                        .and_then(|v| v.as_str())
                        .unwrap_or("-");
                    let target_port = item
                        .get("target_port")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let status = item.get("status").and_then(|v| v.as_str()).unwrap_or("-");
                    let desc = item
                        .get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    let bind_str = format!("{bind_addr}:{bind_port}");
                    let target_str = if fwd_type == "dynamic" {
                        "SOCKS5".to_string()
                    } else {
                        format!("{target_host}:{target_port}")
                    };

                    println!(
                        "  {:<10} {:<8} {:<24} {:<24} {:<10} {}",
                        short_session,
                        fwd_type,
                        bind_str,
                        target_str,
                        status,
                        sanitize_display(desc)
                    );
                }
            }
        }
    }

    /// Print health status.
    pub fn print_health(&self, value: &Value, single: bool) {
        match self {
            Self::Json => {
                println!("{}", serde_json::to_string(value).unwrap_or_default());
            }
            Self::Human => {
                if single {
                    // Single session health (QuickHealthCheck)
                    let status = value
                        .get("status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let latency = value.get("latency_ms").and_then(|v| v.as_u64());
                    let message = value.get("message").and_then(|v| v.as_str()).unwrap_or("");
                    let session_id = value
                        .get("session_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("-");

                    let status_icon = match status {
                        "healthy" => "●",
                        "degraded" => "◐",
                        "unresponsive" => "○",
                        "disconnected" => "✕",
                        _ => "?",
                    };

                    let latency_str = latency
                        .map(|l| format!("{l}ms"))
                        .unwrap_or_else(|| "-".to_string());

                    println!("{status_icon} {session_id}");
                    println!("  Status:    {status}");
                    println!("  Latency:   {latency_str}");
                    println!("  Message:   {message}");
                } else {
                    // All sessions health (HashMap<String, QuickHealthCheck>)
                    let obj = value.as_object();
                    if obj.map(|o| o.is_empty()).unwrap_or(true) {
                        println!("No active sessions with health data");
                        return;
                    }

                    println!(
                        "  {:<14} {:<14} {:<10} {}",
                        "SESSION", "STATUS", "LATENCY", "MESSAGE"
                    );
                    if let Some(map) = obj {
                        for (session_id, check) in map {
                            let short_id = if session_id.len() > 12 {
                                &session_id[..12]
                            } else {
                                session_id
                            };
                            let status = check
                                .get("status")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown");
                            let latency = check
                                .get("latency_ms")
                                .and_then(|v| v.as_u64())
                                .map(|l| format!("{l}ms"))
                                .unwrap_or_else(|| "-".to_string());
                            let message =
                                check.get("message").and_then(|v| v.as_str()).unwrap_or("");
                            println!(
                                "  {:<14} {:<14} {:<10} {}",
                                short_id, status, latency, message
                            );
                        }
                    }
                }
            }
        }
    }

    /// Print disconnect result.
    pub fn print_disconnect(&self, value: &Value) {
        match self {
            Self::Json => {
                println!("{}", serde_json::to_string(value).unwrap_or_default());
            }
            Self::Human => {
                let success = value
                    .get("success")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let session_id = value
                    .get("session_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                if success {
                    println!("Disconnected session: {session_id}");
                } else {
                    let error = value
                        .get("error")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unknown error");
                    println!("Failed to disconnect: {error}");
                }
            }
        }
    }

    /// Print version information.
    pub fn print_version(&self) {
        let version = env!("CARGO_PKG_VERSION");
        match self {
            Self::Json => {
                println!(
                    "{}",
                    serde_json::to_string(&serde_json::json!({
                        "cli_version": version
                    }))
                    .unwrap_or_default()
                );
            }
            Self::Human => {
                println!("oxt {version}");
            }
        }
    }

    /// Print config list (groups with connection counts).
    pub fn print_config_list(&self, value: &Value) {
        match self {
            Self::Json => {
                println!("{}", serde_json::to_string(value).unwrap_or_default());
            }
            Self::Human => {
                let total = value
                    .get("total_connections")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let groups = value
                    .get("groups")
                    .and_then(|v| v.as_array())
                    .map(|a| a.as_slice())
                    .unwrap_or(&[]);

                println!("Saved connections: {total}");
                if groups.is_empty() {
                    return;
                }
                println!();
                println!("  {:<24} {}", "GROUP", "COUNT");
                for group in groups {
                    let name = group.get("name").and_then(|v| v.as_str()).unwrap_or("-");
                    let count = group.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
                    println!("  {:<24} {}", sanitize_display(name), count);
                }
            }
        }
    }

    /// Print config get (connection details).
    pub fn print_config_get(&self, value: &Value) {
        match self {
            Self::Json => {
                println!("{}", serde_json::to_string(value).unwrap_or_default());
            }
            Self::Human => {
                let name = value.get("name").and_then(|v| v.as_str()).unwrap_or("-");
                let host = value.get("host").and_then(|v| v.as_str()).unwrap_or("-");
                let port = value.get("port").and_then(|v| v.as_u64()).unwrap_or(22);
                let user = value
                    .get("username")
                    .and_then(|v| v.as_str())
                    .unwrap_or("-");
                let auth = value
                    .get("auth_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("-");
                let group = value
                    .get("group")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(none)");
                let key_path = value.get("key_path").and_then(|v| v.as_str());

                println!("{}", sanitize_display(name));
                println!("  Host:       {}:{port}", sanitize_display(host));
                println!("  User:       {}", sanitize_display(user));
                println!("  Auth:       {auth}");
                if let Some(kp) = key_path {
                    println!("  Key:        {kp}");
                }
                println!("  Group:      {group}");

                // Proxy chain
                if let Some(chain) = value.get("proxy_chain").and_then(|v| v.as_array()) {
                    if !chain.is_empty() {
                        println!("  Proxy hops:");
                        for hop in chain {
                            let h = hop.get("host").and_then(|v| v.as_str()).unwrap_or("-");
                            let p = hop.get("port").and_then(|v| v.as_u64()).unwrap_or(22);
                            let u = hop.get("username").and_then(|v| v.as_str()).unwrap_or("-");
                            println!("    → {u}@{h}:{p}");
                        }
                    }
                }

                // Options
                if let Some(opts) = value.get("options") {
                    let ka = opts
                        .get("keep_alive_interval")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let comp = opts
                        .get("compression")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    if ka > 0 || comp {
                        println!("  Options:");
                        if ka > 0 {
                            println!("    Keep-alive:   {ka}s");
                        }
                        if comp {
                            println!("    Compression:  on");
                        }
                    }
                }
            }
        }
    }

    /// Print forward create/delete result.
    pub fn print_forward_result(&self, value: &Value) {
        match self {
            Self::Json => {
                println!("{}", serde_json::to_string(value).unwrap_or_default());
            }
            Self::Human => {
                let success = value
                    .get("success")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                if success {
                    if let Some(fwd) = value.get("forward") {
                        let ftype = fwd
                            .get("forward_type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("-");
                        let bind = format!(
                            "{}:{}",
                            fwd.get("bind_address")
                                .and_then(|v| v.as_str())
                                .unwrap_or("127.0.0.1"),
                            fwd.get("bind_port").and_then(|v| v.as_u64()).unwrap_or(0)
                        );
                        let target = if ftype == "dynamic" {
                            "SOCKS5".to_string()
                        } else {
                            format!(
                                "{}:{}",
                                fwd.get("target_host")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("-"),
                                fwd.get("target_port").and_then(|v| v.as_u64()).unwrap_or(0)
                            )
                        };
                        let id = fwd.get("id").and_then(|v| v.as_str()).unwrap_or("-");
                        println!("Forward created: {ftype} {bind} → {target}");
                        println!("  ID: {id}");
                    } else if let Some(fwd_id) = value.get("forward_id").and_then(|v| v.as_str()) {
                        println!("Forward removed: {fwd_id}");
                    } else {
                        println!("Success");
                    }
                } else {
                    let error = value
                        .get("error")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unknown error");
                    eprintln!("Failed: {error}");
                }
            }
        }
    }

    /// Print AI response (non-streaming).
    pub fn print_ai_response(&self, value: &Value) {
        match self {
            Self::Json => {
                println!("{}", serde_json::to_string(value).unwrap_or_default());
            }
            Self::Human => {
                if let Some(text) = value.get("text").and_then(|v| v.as_str()) {
                    println!("{text}");
                } else if let Some(err) = value.get("error").and_then(|v| v.as_str()) {
                    eprintln!("AI error: {err}");
                }
            }
        }
    }

    /// Print connect result.
    pub fn print_connect_result(&self, value: &Value) {
        match self {
            Self::Json => {
                println!("{}", serde_json::to_string(value).unwrap_or_default());
            }
            Self::Human => {
                let success = value
                    .get("success")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if success {
                    let name = value.get("name").and_then(|v| v.as_str()).unwrap_or("-");
                    println!("Connecting to {}...", sanitize_display(name));
                } else {
                    let error = value
                        .get("error")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unknown error");
                    eprintln!("Failed: {error}");
                }
            }
        }
    }

    /// Print SFTP directory listing.
    pub fn print_sftp_ls(&self, value: &Value) {
        match self {
            Self::Json => {
                println!("{}", serde_json::to_string(value).unwrap_or_default());
            }
            Self::Human => {
                let path = value.get("path").and_then(|v| v.as_str()).unwrap_or(".");
                let entries = value
                    .get("entries")
                    .and_then(|v| v.as_array())
                    .map(|a| a.as_slice())
                    .unwrap_or(&[]);

                println!("{path}  ({} entries)", entries.len());
                if entries.is_empty() {
                    return;
                }

                println!("  {:<10} {:<10} {:<8} {}", "PERMS", "SIZE", "TYPE", "NAME");
                for entry in entries {
                    let name = entry.get("name").and_then(|v| v.as_str()).unwrap_or("-");
                    let file_type = entry.get("type").and_then(|v| v.as_str()).unwrap_or("-");
                    let size = entry.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
                    let permissions = entry
                        .get("permissions")
                        .and_then(|v| v.as_str())
                        .unwrap_or("---");
                    let is_symlink = entry
                        .get("is_symlink")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    let type_char = match file_type {
                        "Directory" => "d",
                        "Symlink" => "l",
                        "File" => "-",
                        _ => "?",
                    };
                    let size_str = format_file_size(size);
                    let display_name = if is_symlink {
                        format!("{} →", sanitize_display(name))
                    } else {
                        sanitize_display(name)
                    };

                    println!(
                        "  {}{:<9} {:<10} {:<8} {}",
                        type_char, permissions, size_str, file_type, display_name
                    );
                }
            }
        }
    }

    /// Print SFTP transfer result (download or upload).
    pub fn print_sftp_transfer(&self, value: &Value, verb: &str) {
        match self {
            Self::Json => {
                println!("{}", serde_json::to_string(value).unwrap_or_default());
            }
            Self::Human => {
                let remote = value
                    .get("remote_path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let local = value
                    .get("local_path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let bytes = value.get("bytes").and_then(|v| v.as_u64()).unwrap_or(0);
                let size_str = format_file_size(bytes);
                println!("{verb}: {remote} ↔ {local} ({size_str})");
            }
        }
    }

    /// Print importable SSH config hosts.
    pub fn print_import_list(&self, value: &Value) {
        match self {
            Self::Json => {
                println!("{}", serde_json::to_string(value).unwrap_or_default());
            }
            Self::Human => {
                let items = value.as_array().map(|a| a.as_slice()).unwrap_or(&[]);
                if items.is_empty() {
                    println!("No hosts found in ~/.ssh/config");
                    return;
                }

                println!(
                    "  {:<20} {:<24} {:<10} {:<6} {}",
                    "ALIAS", "HOSTNAME", "USER", "PORT", "STATUS"
                );
                for item in items {
                    let alias = item.get("alias").and_then(|v| v.as_str()).unwrap_or("-");
                    let hostname = item.get("hostname").and_then(|v| v.as_str()).unwrap_or("-");
                    let user = item.get("user").and_then(|v| v.as_str()).unwrap_or("-");
                    let port = item.get("port").and_then(|v| v.as_u64()).unwrap_or(22);
                    let imported = item
                        .get("already_imported")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let status = if imported { "imported" } else { "available" };

                    println!(
                        "  {:<20} {:<24} {:<10} {:<6} {}",
                        sanitize_display(alias),
                        sanitize_display(hostname),
                        sanitize_display(user),
                        port,
                        status
                    );
                }
            }
        }
    }

    /// Print import result summary.
    pub fn print_import_result(&self, value: &Value) {
        match self {
            Self::Json => {
                println!("{}", serde_json::to_string(value).unwrap_or_default());
            }
            Self::Human => {
                let imported = value.get("imported").and_then(|v| v.as_u64()).unwrap_or(0);
                let skipped = value.get("skipped").and_then(|v| v.as_u64()).unwrap_or(0);
                let errors = value
                    .get("errors")
                    .and_then(|v| v.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);

                println!("Imported: {imported}, Skipped: {skipped}, Errors: {errors}");

                if let Some(errs) = value.get("errors").and_then(|v| v.as_array()) {
                    for err in errs {
                        if let Some(msg) = err.as_str() {
                            eprintln!("  ✕ {msg}");
                        }
                    }
                }
            }
        }
    }
}

fn format_duration(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        format!("{h}h {m}m")
    }
}

/// Check if stdout is connected to a terminal (not piped).
fn is_terminal_stdout() -> bool {
    std::io::stdout().is_terminal()
}

/// Strip ANSI escape sequences and control characters from a string
/// to prevent terminal injection attacks via crafted connection names.
fn sanitize_display(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip ESC sequence: ESC [ ... final_byte
            if chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                              // Consume until we hit a letter (final byte of CSI sequence)
                for c2 in chars.by_ref() {
                    if c2.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else if c >= ' ' || c == '\t' {
            result.push(c);
        }
        // Drop other control characters
    }
    result
}

/// Format a file size in human-readable form.
fn format_file_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.1}G", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1}M", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1}K", bytes as f64 / KB as f64)
    } else {
        format!("{bytes}B")
    }
}
