fn ai_tool_requires_ui_thread(
    snapshot: &AiOrchestratorRuntimeSnapshot,
    tool_name: &str,
    args: &serde_json::Value,
) -> bool {
    if matches!(tool_name, "connect_target" | "send_terminal_input" | "open_app_surface" | "remember_preference") {
        return true;
    }
    if tool_name == "write_resource" {
        return args
            .get("resource")
            .and_then(serde_json::Value::as_str)
            == Some("settings");
    }
    if tool_name == "run_command"
        && let Some(target_id) = args.get("target_id").and_then(serde_json::Value::as_str)
    {
        return snapshot
            .targets
            .iter()
            .any(|target| target.id == target_id && target.kind == "terminal-session");
    }
    false
}

#[derive(Clone, Debug)]
struct AiActionResultLite {
    ok: bool,
    summary: String,
    output: String,
    data: serde_json::Value,
    error_code: Option<String>,
    error_message: Option<String>,
    risk: &'static str,
    target: Option<AiOrchestratorTarget>,
    targets: Vec<AiOrchestratorTarget>,
}

impl AiActionResultLite {
    fn with_target(mut self, target: AiOrchestratorTarget) -> Self {
        self.target = Some(target);
        self
    }

    fn with_targets(mut self, targets: Vec<AiOrchestratorTarget>) -> Self {
        self.targets = targets;
        self
    }

    fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = data;
        self
    }

    fn with_optional_target(mut self, target: Option<AiOrchestratorTarget>) -> Self {
        self.target = target;
        self
    }
}

async fn run_local_ai_command(command: &str, timeout_secs: u64, target: &AiOrchestratorTarget) -> AiActionResultLite {
    let mut process = tokio::process::Command::new(if cfg!(target_os = "windows") { "cmd" } else { "sh" });
    if cfg!(target_os = "windows") {
        process.arg("/C").arg(command);
    } else {
        process.arg("-lc").arg(command);
    }
    match tokio::time::timeout(Duration::from_secs(timeout_secs), process.output()).await {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let exit_code = output.status.code();
            let body = [
                stdout.to_string(),
                (!stderr.trim().is_empty()).then(|| format!("[stderr]\n{stderr}")).unwrap_or_default(),
                format!("[exit_code: {}]", exit_code.map(|code| code.to_string()).unwrap_or_else(|| "unknown".to_string())),
            ]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
            AiActionResultLite {
                ok: output.status.success(),
                summary: if output.status.success() {
                    "Local command completed.".to_string()
                } else {
                    format!("Local command exited with {}.", exit_code.map(|code| code.to_string()).unwrap_or_else(|| "unknown".to_string()))
                },
                output: body,
                data: serde_json::json!({ "exitCode": exit_code }),
                error_code: (!output.status.success()).then(|| "local_command_failed".to_string()),
                error_message: (!output.status.success()).then(|| format!("Exit code: {}", exit_code.map(|code| code.to_string()).unwrap_or_else(|| "unknown".to_string()))),
                risk: "execute",
                target: Some(target.clone()),
                targets: Vec::new(),
            }
        }
        Ok(Err(error)) => AiActionResultLite {
            ok: false,
            summary: "Local command failed.".to_string(),
            output: error.to_string(),
            data: serde_json::Value::Null,
            error_code: Some("local_command_error".to_string()),
            error_message: Some(error.to_string()),
            risk: "execute",
            target: Some(target.clone()),
            targets: Vec::new(),
        },
        Err(_) => AiActionResultLite {
            ok: false,
            summary: "Local command timed out.".to_string(),
            output: "Command timed out.".to_string(),
            data: serde_json::json!({ "timedOut": true }),
            error_code: Some("local_command_timeout".to_string()),
            error_message: Some("Command timed out.".to_string()),
            risk: "execute",
            target: Some(target.clone()),
            targets: Vec::new(),
        },
    }
}

fn target_in_ai_view(target: &AiOrchestratorTarget, view: &str) -> bool {
    match view {
        "connections" => matches!(target.kind.as_str(), "saved-connection" | "ssh-node"),
        "live_sessions" => {
            matches!(target.kind.as_str(), "terminal-session" | "sftp-session")
                || (target.kind == "ssh-node" && target.state == "connected")
        }
        "app_surfaces" => matches!(target.kind.as_str(), "settings" | "app-surface" | "local-shell" | "rag-index"),
        "files" => {
            matches!(target.kind.as_str(), "sftp-session" | "ide-workspace" | "rag-index")
                || (target.kind == "ssh-node" && target.capabilities.iter().any(|capability| capability.starts_with("filesystem.")))
        }
        "all" => true,
        _ => true,
    }
}

fn view_for_ai_intent(intent: &str) -> &'static str {
    match intent {
        "command" | "terminal" => "live_sessions",
        "settings" | "app_surface" | "local" => "app_surfaces",
        "file" | "sftp" | "knowledge" => "files",
        "connection" | "status" | "unknown" | _ => "connections",
    }
}

fn target_json(target: &AiOrchestratorTarget) -> serde_json::Value {
    serde_json::json!({
        "id": target.id,
        "kind": target.kind,
        "label": target.label,
        "state": target.state,
        "capabilities": target.capabilities,
        "refs": target.refs,
        "metadata": target.metadata,
    })
}

fn risk_to_capability(risk: &str) -> Option<&'static str> {
    match risk {
        "read" => Some("state.list"),
        "write" => Some("filesystem.write"),
        "execute" => Some("command.run"),
        "interactive" => Some("terminal.send"),
        _ => None,
    }
}

fn trim_tail_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let tail = value.chars().rev().take(max_chars).collect::<Vec<_>>();
    tail.into_iter().rev().collect()
}

fn truncate_for_model(value: String, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value;
    }
    let head = value.chars().take(max_chars).collect::<String>();
    format!("{head}\n\n[truncated]")
}

fn ai_hash_text_content(content: &str, encoding: &str) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(encoding.as_bytes());
    hasher.update([0]);
    hasher.update(content.as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

fn ai_remote_directory_prefixes(path: &str) -> Vec<String> {
    let absolute = path.starts_with('/');
    path.split('/')
        .filter(|part| !part.is_empty())
        .scan(Vec::<&str>::new(), |parts, part| {
            parts.push(part);
            let joined = parts.join("/");
            Some(if absolute {
                format!("/{joined}")
            } else {
                joined
            })
        })
        .collect()
}

fn ai_transfer_name(local_path: &str, remote_path: &str) -> String {
    std::path::Path::new(local_path)
        .file_name()
        .or_else(|| std::path::Path::new(remote_path).file_name())
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Directory transfer".to_string())
}
