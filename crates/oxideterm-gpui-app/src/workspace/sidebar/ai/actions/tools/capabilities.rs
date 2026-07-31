use super::*;
use sha2::Digest as _;

pub(in crate::workspace) fn ai_tool_requires_ui_thread(
    tool_name: &str,
    _args: &serde_json::Value,
) -> bool {
    // Every application tool crosses the same UI-owned broker. Background
    // provider tasks therefore never execute against their frozen snapshots.
    oxideterm_ai::is_orchestrator_tool_name(tool_name)
}

/// Rejects the removed v1 authority field before any canonical v2 dispatch.
/// This is defense in depth alongside strict schemas and argument parsing.
pub(in crate::workspace) fn ai_rejects_legacy_live_target_argument(
    tool_name: &str,
    args: &serde_json::Value,
) -> bool {
    matches!(
        tool_name,
        "read_resource"
            | "write_resource"
            | "transfer_resource"
            | "open_app_surface"
            | "get_state"
    ) && args.get("target_id").is_some()
}

/// Identifies a stable resource only after its typed wire reference has been
/// parsed and checked against the operation it is allowed to perform.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum AiStableResourceOperation {
    SavedConnection(oxideterm_ai::StableResourceRef),
    Settings,
    Rag,
    AppSurface(String),
}

/// Classifies the exact live-owner capability required by a v2 resource tool.
/// A resource family cannot be widened by selecting a handle of another kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum AiLiveResourceOperation {
    SftpRead,
    SftpWrite,
    SftpStartTransfer,
    IdeRead,
    IdeWrite,
}

impl AiLiveResourceOperation {
    pub(in crate::workspace) const fn capability(self) -> oxideterm_ai::RuntimeCapability {
        match self {
            Self::SftpRead => oxideterm_ai::RuntimeCapability::SftpRead,
            Self::SftpWrite => oxideterm_ai::RuntimeCapability::SftpWrite,
            Self::SftpStartTransfer => oxideterm_ai::RuntimeCapability::SftpStartTransfer,
            Self::IdeRead => oxideterm_ai::RuntimeCapability::IdeRead,
            Self::IdeWrite => oxideterm_ai::RuntimeCapability::IdeWrite,
        }
    }

    pub(in crate::workspace) const fn requires_ide_owner(self) -> bool {
        matches!(self, Self::IdeRead | Self::IdeWrite)
    }
}

pub(in crate::workspace) fn ai_live_resource_operation(
    tool_name: &str,
    args: &serde_json::Value,
) -> Result<AiLiveResourceOperation, oxideterm_ai::RuntimeValidationError> {
    let resource = args.get("resource").and_then(serde_json::Value::as_str);
    let operation = match (tool_name, resource) {
        ("read_resource", Some("file" | "directory" | "sftp")) => {
            AiLiveResourceOperation::SftpRead
        }
        ("read_resource", Some("ide")) => AiLiveResourceOperation::IdeRead,
        ("write_resource", Some("file")) => AiLiveResourceOperation::SftpWrite,
        ("write_resource", Some("ide")) => AiLiveResourceOperation::IdeWrite,
        ("transfer_resource", _) => AiLiveResourceOperation::SftpStartTransfer,
        _ => {
            return Err(oxideterm_ai::RuntimeValidationError::new(
                oxideterm_ai::RuntimeValidationFailure::CapabilityUnavailable,
            ));
        }
    };
    Ok(operation)
}

pub(in crate::workspace) fn ai_stable_resource_operation(
    tool_name: &str,
    args: &serde_json::Value,
) -> Result<AiStableResourceOperation, oxideterm_ai::RuntimeValidationError> {
    let resource_ref = args
        .get("resource_ref")
        .cloned()
        .and_then(|value| serde_json::from_value::<oxideterm_ai::StableResourceRef>(value).ok())
        .ok_or_else(|| {
            oxideterm_ai::RuntimeValidationError::new(
                oxideterm_ai::RuntimeValidationFailure::CapabilityUnavailable,
            )
        })?;

    let operation = match tool_name {
        "connect_target"
            if resource_ref.kind() == oxideterm_ai::StableResourceKind::SavedConnection =>
        {
            AiStableResourceOperation::SavedConnection(resource_ref)
        }
        "read_resource"
            if args.get("resource").and_then(serde_json::Value::as_str) == Some("settings")
                && resource_ref.kind() == oxideterm_ai::StableResourceKind::SettingsScope
                && resource_ref.id() == "app" =>
        {
            AiStableResourceOperation::Settings
        }
        "read_resource"
            if args.get("resource").and_then(serde_json::Value::as_str) == Some("rag")
                && resource_ref.kind() == oxideterm_ai::StableResourceKind::RagIndex
                && resource_ref.id() == "default" =>
        {
            AiStableResourceOperation::Rag
        }
        "write_resource"
            if args.get("resource").and_then(serde_json::Value::as_str) == Some("settings")
                && resource_ref.kind() == oxideterm_ai::StableResourceKind::SettingsScope
                && resource_ref.id() == "app" =>
        {
            AiStableResourceOperation::Settings
        }
        "open_app_surface"
            if resource_ref.kind() == oxideterm_ai::StableResourceKind::AppSurface =>
        {
            AiStableResourceOperation::AppSurface(resource_ref.id().to_string())
        }
        _ => {
            return Err(oxideterm_ai::RuntimeValidationError::new(
                oxideterm_ai::RuntimeValidationFailure::CapabilityUnavailable,
            ));
        }
    };

    Ok(operation)
}

#[cfg(windows)]
pub(in crate::workspace) const AI_LOCAL_COMMAND_CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Clone, Debug)]
pub(in crate::workspace) struct AiActionResultLite {
    pub(in crate::workspace) ok: bool,
    pub(in crate::workspace) summary: String,
    pub(in crate::workspace) output: String,
    pub(in crate::workspace) data: serde_json::Value,
    pub(in crate::workspace) error_code: Option<String>,
    pub(in crate::workspace) error_message: Option<String>,
    pub(in crate::workspace) risk: &'static str,
    pub(in crate::workspace) target: Option<AiOrchestratorTarget>,
    pub(in crate::workspace) targets: Vec<AiOrchestratorTarget>,
    pub(in crate::workspace) next_actions: Vec<serde_json::Value>,
    pub(in crate::workspace) observations: Vec<String>,
    pub(in crate::workspace) verified: Option<bool>,
    pub(in crate::workspace) state_version: Option<String>,
}

impl AiActionResultLite {
    pub(in crate::workspace) fn with_target(mut self, target: AiOrchestratorTarget) -> Self {
        self.target = Some(target);
        self
    }

    pub(in crate::workspace) fn with_targets(mut self, targets: Vec<AiOrchestratorTarget>) -> Self {
        self.targets = targets;
        self
    }

    pub(in crate::workspace) fn with_next_actions(
        mut self,
        next_actions: Vec<serde_json::Value>,
    ) -> Self {
        self.next_actions = next_actions;
        self
    }

    pub(in crate::workspace) fn with_verified(mut self, verified: bool) -> Self {
        self.verified = Some(verified);
        self
    }


    pub(in crate::workspace) fn with_optional_target(
        mut self,
        target: Option<AiOrchestratorTarget>,
    ) -> Self {
        self.target = target;
        self
    }

    pub(in crate::workspace) fn with_state_version(
        mut self,
        state_version: impl Into<String>,
    ) -> Self {
        self.state_version = Some(state_version.into());
        self
    }
}

pub(in crate::workspace) async fn run_local_ai_command(
    command: &str,
    cwd: Option<&str>,
    timeout_secs: u64,
    dangerous_command_approved: bool,
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
            target: None,
            targets: Vec::new(),
            next_actions: Vec::new(),
            observations: Vec::new(),
            verified: None,
            state_version: None,
        };
    }
    let mut process = tokio::process::Command::new(if cfg!(target_os = "windows") {
        "cmd"
    } else {
        "sh"
    });
    // Dropping a cancelled tool future must also terminate its local child.
    process.kill_on_drop(true);
    configure_ai_local_command_process(&mut process);
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
                error_message: Some("Working directory does not exist.".to_string()),
                risk: "execute",
                target: None,
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
            let stdout = truncate_ai_local_exec_output(&String::from_utf8_lossy(&output.stdout));
            let stderr = truncate_ai_local_exec_output(&String::from_utf8_lossy(&output.stderr));
            let exit_code = output.status.code();
            let has_output = !stdout.trim().is_empty() || !stderr.trim().is_empty();
            let ok = output.status.success() || (exit_code.is_none() && has_output);
            let body = [
                stdout,
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
            .join("\n");
            AiActionResultLite {
                ok,
                summary: if output.status.success() {
                    "Local command completed.".to_string()
                } else if exit_code.is_none() && has_output {
                    "Local command output captured; exit code was not reported.".to_string()
                } else {
                    format!(
                        "Local command exited with {}.",
                        exit_code
                            .map(|code| code.to_string())
                            .unwrap_or_else(|| "unknown".to_string())
                    )
                },
                output: body,
                data: serde_json::json!({
                    "exitCode": exit_code,
                    "timedOut": false,
                    "executionState": if output.status.success() { "completed" } else { "output_captured" },
                    "visibleInTerminal": false,
                }),
                error_code: (!ok).then(|| "local_command_failed".to_string()),
                error_message: (!ok).then(|| "The local command failed.".to_string()),
                risk: "execute",
                target: None,
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
            error_message: Some("The local command could not be started.".to_string()),
            risk: "execute",
            target: None,
            targets: Vec::new(),
            next_actions: Vec::new(),
            observations: Vec::new(),
            verified: None,
            state_version: None,
        },
        Err(_) => AiActionResultLite {
            ok: false,
            summary: "Local command timed out.".to_string(),
            output: format!(
                "[stderr]\nCommand timed out after {timeout_secs}s\n[exit_code: unknown]"
            ),
            data: serde_json::json!({
                "exitCode": serde_json::Value::Null,
                "timedOut": true,
                "executionState": "timeout",
                "visibleInTerminal": false,
            }),
            error_code: Some("local_command_timeout".to_string()),
            error_message: Some("Command timed out.".to_string()),
            risk: "execute",
            target: None,
            targets: Vec::new(),
            next_actions: Vec::new(),
            observations: Vec::new(),
            verified: None,
            state_version: None,
        },
    }
}

pub(in crate::workspace) fn configure_ai_local_command_process(
    process: &mut tokio::process::Command,
) {
    #[cfg(windows)]
    {
        // AI local commands capture stdout/stderr in the app. Hide the bridge
        // shell so cmd.exe, pwsh.exe, and child console programs do not flash.
        process.creation_flags(AI_LOCAL_COMMAND_CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        let _ = process;
    }
}

pub(in crate::workspace) fn ai_local_exec_timeout_secs(timeout_secs: u64) -> u64 {
    // Tauri's local_exec_command caps the backend timeout at 60 seconds even if
    // a caller bypasses the tool schema bounds.
    timeout_secs.min(60)
}

pub(in crate::workspace) fn ai_memory_settings_json(
    enabled: bool,
    content: &str,
    entries: &[oxideterm_settings::AiMemoryEntry],
) -> serde_json::Value {
    // Keep the legacy content field while exposing the itemized model.
    serde_json::json!({
        "enabled": enabled,
        "content": content,
        "entries": entries.iter().map(ai_memory_entry_json).collect::<Vec<_>>(),
    })
}

pub(in crate::workspace) fn ai_memory_content(memory: &serde_json::Value) -> &str {
    memory
        .get("content")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
}

pub(in crate::workspace) fn ai_memory_trimmed_content(memory: &serde_json::Value) -> &str {
    ai_memory_content(memory).trim()
}

pub(in crate::workspace) fn ai_tool_verified_default(
    ok: bool,
    error_message: Option<&str>,
) -> bool {
    // Tauri marks an implicit result as verified only when it succeeded and did
    // not carry an error object.
    ok && error_message.is_none()
}

pub(in crate::workspace) fn ai_run_command_preflight_risk() -> &'static str {
    // Tauri validates run_command target readiness and command text before the
    // terminal capability switches the action risk to interactive.
    "execute"
}

pub(in crate::workspace) fn truncate_ai_local_exec_output(value: &str) -> String {
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

pub(in crate::workspace) fn ai_shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub(in crate::workspace) fn ai_command_with_cwd(command: &str, cwd: Option<&str>) -> String {
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



pub(in crate::workspace) fn target_in_ai_view(target: &AiOrchestratorTarget, view: &str) -> bool {
    match view {
        "connections" => matches!(target.kind.as_str(), "saved-connection" | "ssh-node"),
        "live_sessions" => {
            matches!(target.kind.as_str(), "terminal-session" | "sftp-session")
                || (target.kind == "ssh-node" && target.state == "connected")
        }
        "app_surfaces" => matches!(
            target.kind.as_str(),
            "settings" | "app-surface" | "local-shell" | "rag-index"
        ),
        "files" => {
            matches!(
                target.kind.as_str(),
                "sftp-session" | "ide-workspace" | "rag-index"
            ) || (target.kind == "ssh-node"
                && target
                    .capabilities
                    .iter()
                    .any(|capability| capability.starts_with("filesystem.")))
        }
        "all" => true,
        _ => true,
    }
}

pub(in crate::workspace) fn normalized_ai_target_view(view: Option<&str>) -> &'static str {
    match view {
        Some("connections") => "connections",
        Some("live_sessions") => "live_sessions",
        Some("app_surfaces") => "app_surfaces",
        Some("files") => "files",
        Some("all") => "all",
        _ => "connections",
    }
}

pub(in crate::workspace) fn target_matches_ai_query(
    target: &AiOrchestratorTarget,
    query: &str,
) -> bool {
    if query.is_empty() {
        return true;
    }
    // Discovery may rank presentation data, but it must never turn a leaked
    // node, tab, pane, or session id back into current authority.
    if target.label.to_lowercase().contains(query) {
        return true;
    }
    if target
        .metadata
        .get("host")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|host| host.to_lowercase().contains(query))
    {
        return true;
    }
    // Saved UUIDs are durable identities. Match them exactly so UUID
    // fragments cannot accidentally select an unrelated current resource.
    target
        .refs
        .get("connectionId")
        .is_some_and(|connection_id| connection_id.eq_ignore_ascii_case(query))
}

pub(in crate::workspace) fn normalized_ai_query(query: Option<&str>) -> String {
    // Tauri trims discovery queries before filtering targets.
    query.unwrap_or("").trim().to_lowercase()
}

pub(in crate::workspace) fn view_for_ai_intent(intent: &str) -> &'static str {
    match intent {
        "command" | "terminal" => "live_sessions",
        "settings" | "app_surface" | "local" => "app_surfaces",
        "file" | "sftp" | "knowledge" => "files",
        "connection" | "status" | "unknown" | _ => "connections",
    }
}

pub(in crate::workspace) fn target_matches_active_context(
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

pub(in crate::workspace) fn normalized_ai_intent(intent: Option<&str>) -> Option<&'static str> {
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

pub(in crate::workspace) fn normalized_ai_select_target_kind(
    kind: Option<&str>,
) -> Option<&'static str> {
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


pub(in crate::workspace) fn ai_rag_query_arg(args: &serde_json::Value) -> &str {
    // Tauri uses `options.query ?? options.path ?? ''` for RAG reads and does
    // not trim the selected string before passing it to ragSearch.
    args.get("query")
        .or_else(|| args.get("path"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
}

pub(in crate::workspace) fn is_ai_command_like_query(query: &str) -> bool {
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
        "pwd",
        "ls",
        "cd",
        "cat",
        "tail",
        "head",
        "grep",
        "find",
        "ps",
        "top",
        "htop",
        "df",
        "du",
        "free",
        "whoami",
        "id",
        "uname",
        "docker",
        "kubectl",
        "systemctl",
        "journalctl",
        "git",
        "npm",
        "pnpm",
        "yarn",
        "cargo",
        "python",
        "node",
        "ssh",
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

pub(in crate::workspace) struct AiConnectionCounts {
    pub(in crate::workspace) total: usize,
    pub(in crate::workspace) saved: usize,
    pub(in crate::workspace) live: usize,
    pub(in crate::workspace) link_down: usize,
    pub(in crate::workspace) error: usize,
}

pub(in crate::workspace) fn ai_connection_counts(
    targets: &[AiOrchestratorTarget],
) -> AiConnectionCounts {
    let connections = targets
        .iter()
        .filter(|target| target_in_ai_view(target, "connections"))
        .collect::<Vec<_>>();
    AiConnectionCounts {
        total: connections.len(),
        saved: connections
            .iter()
            .filter(|target| target.kind == "saved-connection")
            .count(),
        live: connections
            .iter()
            .filter(|target| target.kind == "ssh-node" && target.state == "connected")
            .count(),
        link_down: connections
            .iter()
            .filter(|target| target.kind == "ssh-node" && target.state == "stale")
            .count(),
        error: connections
            .iter()
            .filter(|target| {
                target.kind == "ssh-node"
                    && target
                        .metadata
                        .get("status")
                        .and_then(serde_json::Value::as_str)
                        == Some("error")
            })
            .count(),
    }
}

pub(in crate::workspace) fn ai_background_transfer_state_label(
    state: BackgroundTransferState,
) -> &'static str {
    match state {
        BackgroundTransferState::Pending => "pending",
        BackgroundTransferState::Active => "active",
        BackgroundTransferState::Paused => "paused",
        BackgroundTransferState::Completed => "completed",
        BackgroundTransferState::Cancelled => "cancelled",
        BackgroundTransferState::Error => "error",
    }
}

pub(in crate::workspace) fn ai_transfers_state(
    manager: &SftpTransferManager,
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
                "direction": transfer.direction,
                "state": ai_background_transfer_state_label(transfer.state),
                "size": transfer.size,
                "transferred": transfer.transferred,
                "hasError": transfer.error.is_some(),
                "startTime": transfer.start_time,
                "endTime": transfer.end_time,
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({
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

pub(in crate::workspace) fn ai_health_state(
    snapshot: &AiOrchestratorRuntimeSnapshot,
) -> serde_json::Value {
    snapshot.health_state.clone()
}

pub(in crate::workspace) fn risk_to_capability(risk: &str) -> Option<&'static str> {
    match risk {
        "read" => Some("state.list"),
        "write" => Some("filesystem.write"),
        "execute" => Some("command.run"),
        "interactive" => Some("terminal.send"),
        _ => None,
    }
}

pub(in crate::workspace) fn trim_tail_chars(value: &str, max_chars: usize) -> String {
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

pub(in crate::workspace) fn ai_short_id(value: &str) -> String {
    value.chars().take(8).collect()
}

pub(in crate::workspace) fn truncate_for_model(value: String, max_chars: usize) -> String {
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

pub(in crate::workspace) fn ai_line_count(value: &str) -> usize {
    if value.is_empty() {
        0
    } else {
        value.split('\n').count()
    }
}

pub(in crate::workspace) fn ai_head_tail_preview(value: &str, max_chars: usize) -> String {
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

pub(in crate::workspace) fn prepare_ai_tool_output(
    value: &str,
) -> (String, Option<String>, serde_json::Value, bool) {
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

pub(in crate::workspace) fn ai_next_action_json(
    action: &serde_json::Value,
) -> Option<serde_json::Value> {
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

pub(in crate::workspace) fn ai_hash_text_content(content: &str, encoding: &str) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(encoding.as_bytes());
    hasher.update([0]);
    hasher.update(content.as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

pub(in crate::workspace) fn ai_remote_directory_prefixes(path: &str) -> Vec<String> {
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

pub(in crate::workspace) fn ai_transfer_name(local_path: &str, remote_path: &str) -> String {
    std::path::Path::new(local_path)
        .file_name()
        .or_else(|| std::path::Path::new(remote_path).file_name())
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Directory transfer".to_string())
}
