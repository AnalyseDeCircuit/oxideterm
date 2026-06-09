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
            .any(|target| target.id == target_id && ai_run_command_requires_ui_thread_target(target));
    }
    false
}

fn ai_run_command_requires_ui_thread_target(target: &AiOrchestratorTarget) -> bool {
    matches!(target.kind.as_str(), "terminal-session" | "ssh-node" | "local-shell")
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
    next_actions: Vec<serde_json::Value>,
    observations: Vec<String>,
    verified: Option<bool>,
    state_version: Option<String>,
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

    fn with_next_actions(mut self, next_actions: Vec<serde_json::Value>) -> Self {
        self.next_actions = next_actions;
        self
    }

    fn with_observations(mut self, observations: Vec<String>) -> Self {
        self.observations = observations;
        self
    }

    fn with_verified(mut self, verified: bool) -> Self {
        self.verified = Some(verified);
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

    fn with_state_version(mut self, state_version: impl Into<String>) -> Self {
        self.state_version = Some(state_version.into());
        self
    }
}

async fn run_local_ai_command(
    command: &str,
    cwd: Option<&str>,
    timeout_secs: u64,
    dangerous_command_approved: bool,
    target: &AiOrchestratorTarget,
) -> AiActionResultLite {
    if oxideterm_ai::has_denied_commands(
        "run_command",
        Some(&serde_json::json!({ "command": command })),
    ) && !dangerous_command_approved
    {
        return AiActionResultLite {
            ok: false,
            summary: "Local command failed.".to_string(),
            output: "Command denied for security reasons".to_string(),
            data: serde_json::Value::Null,
            error_code: Some("local_command_error".to_string()),
            error_message: Some("Command denied for security reasons".to_string()),
            risk: "execute",
            target: Some(target.clone()),
            targets: Vec::new(),
            next_actions: Vec::new(),
            observations: Vec::new(),
            verified: None,
            state_version: None,
        };
    }
    let mut process = tokio::process::Command::new(if cfg!(target_os = "windows") { "cmd" } else { "sh" });
    if cfg!(target_os = "windows") {
        process.arg("/C").arg(command);
    } else {
        process.arg("-c").arg(command);
    }
    if let Some(cwd) = cwd.filter(|value| !value.trim().is_empty()) {
        let path = std::path::Path::new(cwd);
        if !path.exists() {
            return AiActionResultLite {
                ok: false,
                summary: "Local command failed.".to_string(),
                output: format!("Working directory does not exist: {cwd}"),
                data: serde_json::Value::Null,
                error_code: Some("local_command_error".to_string()),
                error_message: Some(format!("Working directory does not exist: {cwd}")),
                risk: "execute",
                target: Some(target.clone()),
                targets: Vec::new(),
                next_actions: Vec::new(),
                observations: Vec::new(),
                verified: None,
                state_version: None,
            };
        }
        process.current_dir(path);
    }
    let timeout_secs = ai_local_exec_timeout_secs(timeout_secs);
    match tokio::time::timeout(Duration::from_secs(timeout_secs), process.output()).await {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = truncate_ai_local_exec_output(&stdout);
            let stderr = truncate_ai_local_exec_output(&stderr);
            let exit_code = output.status.code();
            let has_output = !stdout.trim().is_empty() || !stderr.trim().is_empty();
            let ok = output.status.success() || (exit_code.is_none() && has_output);
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
                ok,
                summary: if output.status.success() {
                    "Local command completed.".to_string()
                } else if exit_code.is_none() && has_output {
                    "Local command output captured; exit code was not reported.".to_string()
                } else {
                    format!("Local command exited with {}.", exit_code.map(|code| code.to_string()).unwrap_or_else(|| "unknown".to_string()))
                },
                output: body,
                data: serde_json::json!({
                    "exitCode": exit_code,
                    "timedOut": false,
                    "executionState": if output.status.success() { "completed" } else { "output_captured" },
                    "visibleInTerminal": false,
                }),
                error_code: (!ok).then(|| "local_command_failed".to_string()),
                error_message: (!ok).then(|| format!("Exit code: {}", exit_code.map(|code| code.to_string()).unwrap_or_else(|| "unknown".to_string()))),
                risk: "execute",
                target: Some(target.clone()),
                targets: Vec::new(),
                next_actions: Vec::new(),
                observations: (exit_code.is_none() && has_output)
                    .then(|| "The local command produced output, but the backend did not report an exit code.".to_string())
                    .into_iter()
                    .collect(),
                verified: None,
                state_version: None,
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
            next_actions: Vec::new(),
            observations: Vec::new(),
            verified: None,
            state_version: None,
        },
        Err(_) => AiActionResultLite {
            ok: false,
            summary: "Local command timed out.".to_string(),
            output: format!("[stderr]\nCommand timed out after {timeout_secs}s\n[exit_code: unknown]"),
            data: serde_json::json!({
                "exitCode": serde_json::Value::Null,
                "timedOut": true,
                "executionState": "timeout",
                "visibleInTerminal": false,
            }),
            error_code: Some("local_command_timeout".to_string()),
            error_message: Some("Command timed out.".to_string()),
            risk: "execute",
            target: Some(target.clone()),
            targets: Vec::new(),
            next_actions: Vec::new(),
            observations: Vec::new(),
            verified: None,
            state_version: None,
        },
    }
}

fn ai_local_exec_timeout_secs(timeout_secs: u64) -> u64 {
    // Tauri's local_exec_command caps the backend timeout at 60 seconds even if
    // a caller bypasses the tool schema bounds.
    timeout_secs.min(60)
}

fn ai_memory_settings_json(enabled: bool, content: &str) -> serde_json::Value {
    // Tauri recall_preferences returns settings.ai.memory verbatim, including
    // the enabled flag even when the content is empty.
    serde_json::json!({
        "enabled": enabled,
        "content": content,
    })
}

fn ai_memory_content(memory: &serde_json::Value) -> &str {
    memory
        .get("content")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
}

fn ai_memory_trimmed_content(memory: &serde_json::Value) -> &str {
    ai_memory_content(memory).trim()
}

fn ai_tool_verified_default(ok: bool, error_message: Option<&str>) -> bool {
    // Tauri marks an implicit result as verified only when it succeeded and did
    // not carry an error object.
    ok && error_message.is_none()
}

fn ai_run_command_preflight_risk() -> &'static str {
    // Tauri validates run_command target readiness and command text before the
    // terminal capability switches the action risk to interactive.
    "execute"
}

fn truncate_ai_local_exec_output(value: &str) -> String {
    const MAX_BYTES: usize = 64 * 1024;
    if value.len() <= MAX_BYTES {
        return value.to_string();
    }
    // Tauri truncates local command output at a valid UTF-8 boundary before
    // the AI tool envelope applies its model-facing preview limits.
    let mut end = MAX_BYTES;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...(truncated)", &value[..end])
}

fn ai_shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn ai_command_with_cwd(command: &str, cwd: Option<&str>) -> String {
    match cwd.filter(|value| !value.trim().is_empty()) {
        Some("~") => format!("cd ~ && {command}"),
        Some(cwd) => {
            let target = cwd
                .strip_prefix("~/")
                .filter(|rest| !rest.is_empty())
                .map(|rest| format!("~/{}", ai_shell_single_quote(rest)))
                .unwrap_or_else(|| ai_shell_single_quote(cwd));
            format!("cd {target} && {command}")
        }
        None => command.to_string(),
    }
}

fn ai_command_output(stdout: &str, stderr: &str, exit_code: Option<i32>) -> String {
    [
        stdout.to_string(),
        (!stderr.trim().is_empty())
            .then(|| format!("[stderr]\n{stderr}"))
            .unwrap_or_default(),
        format!(
            "[exit_code: {}]",
            exit_code
                .map(|code| code.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        ),
    ]
    .into_iter()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join("\n")
}

fn ai_target_is_local_terminal(target: &AiOrchestratorTarget) -> bool {
    target
        .metadata
        .get("terminalType")
        .and_then(serde_json::Value::as_str)
        == Some("local_terminal")
}

fn local_terminal_run_target(
    snapshot: &AiOrchestratorRuntimeSnapshot,
) -> Option<AiOrchestratorTarget> {
    let active_tab_id = snapshot.active_tab_id.as_deref();
    let matches_local_terminal = |target: &&AiOrchestratorTarget| {
        target.kind == "terminal-session"
            && target.state == "connected"
            && ai_target_is_local_terminal(target)
    };

    // Prefer the terminal the user is already looking at. If none is active,
    // reuse any connected local terminal before opening another tab.
    snapshot
        .targets
        .iter()
        .find(|target| {
            matches_local_terminal(target)
                && target
                    .refs
                    .get("tabId")
                    .is_some_and(|tab_id| Some(tab_id.as_str()) == active_tab_id)
        })
        .or_else(|| snapshot.targets.iter().find(matches_local_terminal))
        .cloned()
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

fn normalized_ai_target_view(view: Option<&str>) -> &'static str {
    match view {
        Some("connections") => "connections",
        Some("live_sessions") => "live_sessions",
        Some("app_surfaces") => "app_surfaces",
        Some("files") => "files",
        Some("all") => "all",
        _ => "connections",
    }
}

fn target_matches_ai_query(target: &AiOrchestratorTarget, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let haystack = [
        target.id.as_str(),
        target.kind.as_str(),
        target.label.as_str(),
        &target.refs.values().cloned().collect::<Vec<_>>().join(" "),
        &target
            .metadata
            .as_object()
            .map(|object| {
                object
                    .values()
                    .map(ai_js_query_string)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default(),
    ]
    .join(" ")
    .to_lowercase();
    haystack.contains(query)
}

fn normalized_ai_query(query: Option<&str>) -> String {
    // Tauri trims discovery queries before filtering targets.
    query.unwrap_or("").trim().to_lowercase()
}

fn ai_js_query_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::Bool(value) => value.to_string(),
        serde_json::Value::Number(value) => value.to_string(),
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Array(values) => values
            .iter()
            .map(ai_js_query_string)
            .collect::<Vec<_>>()
            .join(","),
        // JavaScript Array.join stringifies plain object metadata this way.
        serde_json::Value::Object(_) => "[object Object]".to_string(),
    }
}

fn target_requires_live_state(target: &AiOrchestratorTarget) -> bool {
    matches!(
        target.kind.as_str(),
        "ssh-node" | "terminal-session" | "sftp-session"
    )
}

fn recovery_actions_for_target(target: &AiOrchestratorTarget) -> Vec<serde_json::Value> {
    match target.kind.as_str() {
        "saved-connection" | "ssh-node" => vec![serde_json::json!({
            "action": "connect_target",
            "args": { "target_id": target.id },
            "reason": "Reconnect or open this SSH target before continuing."
        })],
        "terminal-session" => vec![
            serde_json::json!({
                "action": "observe_terminal",
                "args": { "target_id": target.id },
                "reason": "Check whether the terminal has become ready."
            }),
            serde_json::json!({
                "action": "list_targets",
                "reason": "Find a live terminal or SSH target if this one is stale."
            }),
        ],
        _ => vec![serde_json::json!({
            "action": "list_targets",
            "reason": "Find a currently available target before continuing."
        })],
    }
}

fn ai_ssh_reconnect_failed_next_actions() -> Vec<serde_json::Value> {
    // Tauri returns this specific recovery hint when connect_target cannot
    // reconnect a stale ssh-node target.
    vec![serde_json::json!({
        "action": "list_targets",
        "reason": "Refresh target state before retrying."
    })]
}

fn view_for_ai_intent(intent: &str) -> &'static str {
    match intent {
        "command" | "terminal" => "live_sessions",
        "settings" | "app_surface" | "local" => "app_surfaces",
        "file" | "sftp" | "knowledge" => "files",
        "connection" | "status" | "unknown" | _ => "connections",
    }
}

fn target_matches_active_context(
    target: &AiOrchestratorTarget,
    active_tab_id: Option<&str>,
    active_node_id: Option<&str>,
    active_session_id: Option<&str>,
) -> bool {
    target
        .refs
        .get("tabId")
        .is_some_and(|tab_id| Some(tab_id.as_str()) == active_tab_id)
        || target
            .refs
            .get("sessionId")
            .is_some_and(|session_id| Some(session_id.as_str()) == active_session_id)
        || target
            .refs
            .get("nodeId")
            .is_some_and(|node_id| Some(node_id.as_str()) == active_node_id)
}

fn normalized_ai_intent(intent: Option<&str>) -> Option<&'static str> {
    match intent {
        Some("connection") => Some("connection"),
        Some("command") => Some("command"),
        Some("terminal") => Some("terminal"),
        Some("settings") => Some("settings"),
        Some("file") => Some("file"),
        Some("sftp") => Some("sftp"),
        Some("app_surface") => Some("app_surface"),
        Some("knowledge") => Some("knowledge"),
        Some("status") => Some("status"),
        Some("local") => Some("local"),
        Some("unknown") => Some("unknown"),
        _ => None,
    }
}

fn normalized_ai_select_target_kind(kind: Option<&str>) -> Option<&'static str> {
    match kind {
        Some("all") => Some("all"),
        Some("saved-connection") => Some("saved-connection"),
        Some("ssh-node") => Some("ssh-node"),
        Some("terminal-session") => Some("terminal-session"),
        Some("local-shell") => Some("local-shell"),
        Some("sftp-session") => Some("sftp-session"),
        Some("ide-workspace") => Some("ide-workspace"),
        Some("settings") => Some("settings"),
        Some("app-surface") => Some("app-surface"),
        Some("rag-index") => Some("rag-index"),
        _ => None,
    }
}

fn normalized_ai_resource_kind(resource: Option<&str>) -> &'static str {
    match resource {
        Some("settings") => "settings",
        Some("file") => "file",
        Some("directory") => "directory",
        Some("sftp") => "sftp",
        Some("ide") => "ide",
        Some("rag") => "rag",
        _ => "",
    }
}

fn ai_rag_query_arg(args: &serde_json::Value) -> &str {
    // Tauri uses `options.query ?? options.path ?? ''` for RAG reads and does
    // not trim the selected string before passing it to ragSearch.
    args.get("query")
        .or_else(|| args.get("path"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
}

fn is_ai_command_like_query(query: &str) -> bool {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return false;
    }
    // Keep target selection from treating shell snippets as host names; this
    // mirrors the Tauri orchestrator guardrail that forces a target first.
    let mut words = trimmed.split_whitespace();
    let first_word = words.next().unwrap_or_default();
    let first = if first_word == "sudo" {
        words.next().unwrap_or_default()
    } else {
        first_word
    };
    let command_words = [
        "pwd", "ls", "cd", "cat", "tail", "head", "grep", "find", "ps", "top", "htop", "df",
        "du", "free", "whoami", "id", "uname", "docker", "kubectl", "systemctl", "journalctl",
        "git", "npm", "pnpm", "yarn", "cargo", "python", "node", "ssh",
    ];
    command_words.contains(&first)
        || trimmed.contains(';')
        || trimmed.contains('&')
        || trimmed.contains('|')
        || trimmed.contains('`')
        || trimmed.contains('$')
        || trimmed.contains('<')
        || trimmed.contains('>')
        || trimmed.split_whitespace().skip(1).any(|part| {
            part.strip_prefix("--")
                .or_else(|| part.strip_prefix('-'))
                .and_then(|rest| rest.chars().next())
                .is_some_and(|ch| ch == '_' || ch.is_ascii_alphanumeric())
        })
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

fn tool_result_target_json(target: &AiOrchestratorTarget) -> serde_json::Value {
    let mut metadata = serde_json::Map::new();
    metadata.insert("state".to_string(), serde_json::json!(target.state));
    metadata.insert(
        "capabilities".to_string(),
        serde_json::json!(target.capabilities),
    );
    metadata.insert("refs".to_string(), serde_json::json!(target.refs));
    if let Some(source) = target.metadata.as_object() {
        for (key, value) in source {
            metadata.insert(key.clone(), value.clone());
        }
    }
    serde_json::json!({
        "id": target.id,
        "kind": target.kind,
        "label": target.label,
        "metadata": metadata,
    })
}

fn compact_ai_target_json(target: &AiOrchestratorTarget) -> serde_json::Value {
    serde_json::json!({
        "id": target.id,
        "kind": target.kind,
        "label": target.label,
        "state": target.state,
        "capabilities": target.capabilities,
        "refs": target.refs,
    })
}

fn ai_targets_state(
    targets: &[AiOrchestratorTarget],
    runtime_epoch: &str,
) -> serde_json::Value {
    let view_targets = |view: &str| {
        targets
            .iter()
            .filter(|target| target_in_ai_view(target, view))
            .map(compact_ai_target_json)
            .collect::<Vec<_>>()
    };
    let connections = view_targets("connections");
    let live_sessions = view_targets("live_sessions");
    let app_surfaces = view_targets("app_surfaces");
    let files = view_targets("files");
    serde_json::json!({
        "runtimeEpoch": runtime_epoch,
        "views": {
            "connections": { "count": connections.len(), "targets": connections },
            "live_sessions": { "count": live_sessions.len(), "targets": live_sessions },
            "app_surfaces": { "count": app_surfaces.len(), "targets": app_surfaces },
            "files": { "count": files.len(), "targets": files },
            "all": { "count": targets.len() },
        },
    })
}

fn ai_connections_state(
    targets: &[AiOrchestratorTarget],
    runtime_epoch: &str,
) -> serde_json::Value {
    let connections = targets
        .iter()
        .filter(|target| target_in_ai_view(target, "connections"))
        .collect::<Vec<_>>();
    serde_json::json!({
        "runtimeEpoch": runtime_epoch,
        "total": connections.len(),
        "counts": {
            "saved": connections.iter().filter(|target| target.kind == "saved-connection").count(),
            "live": connections.iter().filter(|target| target.kind == "ssh-node" && target.state == "connected").count(),
            "linkDown": connections.iter().filter(|target| target.kind == "ssh-node" && target.state == "stale").count(),
            "error": connections.iter().filter(|target| {
                target.kind == "ssh-node"
                    && target
                        .metadata
                        .get("status")
                        .and_then(serde_json::Value::as_str)
                        == Some("error")
            }).count(),
        },
        "targets": connections.into_iter().map(compact_ai_target_json).collect::<Vec<_>>(),
    })
}

fn ai_background_transfer_state_label(state: BackgroundTransferState) -> &'static str {
    match state {
        BackgroundTransferState::Pending => "pending",
        BackgroundTransferState::Active => "active",
        BackgroundTransferState::Paused => "paused",
        BackgroundTransferState::Completed => "completed",
        BackgroundTransferState::Cancelled => "cancelled",
        BackgroundTransferState::Error => "error",
    }
}

fn ai_transfers_state(
    manager: &SftpTransferManager,
    runtime_epoch: &str,
) -> serde_json::Value {
    let transfers = manager.list_background_transfers(None);
    let count = |state| {
        transfers
            .iter()
            .filter(|transfer| transfer.state == state)
            .count()
    };
    let active_or_recent = transfers
        .iter()
        .filter(|transfer| {
            matches!(
                transfer.state,
                BackgroundTransferState::Pending
                    | BackgroundTransferState::Active
                    | BackgroundTransferState::Paused
                    | BackgroundTransferState::Error
            )
        })
        .chain(
            transfers
                .iter()
                .filter(|transfer| {
                    matches!(
                        transfer.state,
                        BackgroundTransferState::Completed | BackgroundTransferState::Cancelled
                    )
                })
                .rev()
                .take(5),
        )
        .take(20)
        .map(|transfer| {
            serde_json::json!({
                "id": transfer.id,
                "nodeId": transfer.node_id,
                "name": transfer.name,
                "direction": transfer.direction,
                "state": ai_background_transfer_state_label(transfer.state),
                "size": transfer.size,
                "transferred": transfer.transferred,
                "error": transfer.error,
                "startTime": transfer.start_time,
                "endTime": transfer.end_time,
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "runtimeEpoch": runtime_epoch,
        "total": transfers.len(),
        "counts": {
            "pending": count(BackgroundTransferState::Pending),
            "active": count(BackgroundTransferState::Active),
            "paused": count(BackgroundTransferState::Paused),
            "completed": count(BackgroundTransferState::Completed),
            "cancelled": count(BackgroundTransferState::Cancelled),
            "error": count(BackgroundTransferState::Error),
        },
        "transfers": active_or_recent,
    })
}

fn ai_health_state(snapshot: &AiOrchestratorRuntimeSnapshot) -> serde_json::Value {
    snapshot.health_state.clone()
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
    let omitted = value.chars().count().saturating_sub(max_chars);
    format!(
        "[trimmed {omitted} chars]\n{}",
        tail.into_iter().rev().collect::<String>()
    )
}

fn ai_short_id(value: &str) -> String {
    value.chars().take(8).collect()
}

fn truncate_for_model(value: String, max_chars: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max_chars {
        return value;
    }
    let head = value.chars().take(max_chars).collect::<String>();
    format!(
        "{head}\n[truncated {} chars]",
        char_count.saturating_sub(max_chars)
    )
}

fn ai_line_count(value: &str) -> usize {
    if value.is_empty() {
        0
    } else {
        value.split('\n').count()
    }
}

fn ai_head_tail_preview(value: &str, max_chars: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max_chars {
        return value.to_string();
    }
    let marker = format!(
        "\n\n[output truncated: {} chars omitted; showing head and tail]\n\n",
        char_count.saturating_sub(max_chars)
    );
    let marker_chars = marker.chars().count();
    let available = max_chars.saturating_sub(marker_chars);
    let head_chars = (available * 55).div_ceil(100);
    let tail_chars = available.saturating_sub(head_chars);
    let head = value.chars().take(head_chars).collect::<String>();
    let tail = value
        .chars()
        .rev()
        .take(tail_chars)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    format!("{head}{marker}{tail}")
}

fn prepare_ai_tool_output(value: &str) -> (String, Option<String>, serde_json::Value, bool) {
    const FULL_OUTPUT_MAX_CHARS: usize = 24 * 1024;
    const RAW_OUTPUT_PERSIST_MAX_CHARS: usize = 256 * 1024;
    const MODEL_OUTPUT_PREVIEW_MAX_CHARS: usize = 12_000;

    let char_count = value.chars().count();
    let line_count = ai_line_count(value);
    if char_count <= FULL_OUTPUT_MAX_CHARS {
        return (
            value.to_string(),
            None,
            serde_json::json!({
                "strategy": "full",
                "charCount": char_count,
                "lineCount": line_count,
                "rawOutputStored": false,
            }),
            false,
        );
    }

    let output = ai_head_tail_preview(value, MODEL_OUTPUT_PREVIEW_MAX_CHARS);
    let raw_output_stored = char_count <= RAW_OUTPUT_PERSIST_MAX_CHARS;
    (
        output.clone(),
        raw_output_stored.then(|| value.to_string()),
        serde_json::json!({
            "strategy": "head_tail",
            "charCount": char_count,
            "lineCount": line_count,
            "omittedChars": char_count.saturating_sub(output.chars().count()),
            "rawOutputStored": raw_output_stored,
        }),
        true,
    )
}

fn ai_next_action_json(action: &serde_json::Value) -> Option<serde_json::Value> {
    let action_name = action.get("action").and_then(serde_json::Value::as_str)?;
    let reason = action
        .get("reason")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let mut mapped = serde_json::Map::new();
    mapped.insert("tool".to_string(), serde_json::json!(action_name));
    if let Some(args) = action.get("args") {
        mapped.insert("args".to_string(), args.clone());
    }
    mapped.insert("reason".to_string(), serde_json::json!(reason));
    mapped.insert("priority".to_string(), serde_json::json!("recommended"));
    Some(serde_json::Value::Object(mapped))
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
