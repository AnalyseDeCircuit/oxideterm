//! Resource classification and data shaping, without access to live owners.

use std::collections::BTreeMap;

mod discovery;
pub use discovery::*;
mod snapshot;
pub use snapshot::*;

#[derive(Clone)]
pub struct AiOrchestratorTarget {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub state: String,
    pub capabilities: Vec<String>,
    pub refs: BTreeMap<String, String>,
    pub metadata: serde_json::Value,
    pub terminal_buffer: Option<String>,
    pub terminal_screen: Option<serde_json::Value>,
}

/// Rejects the removed v1 authority field before any canonical v2 dispatch.
/// This is defense in depth alongside strict schemas and argument parsing.
pub fn ai_rejects_legacy_live_target_argument(tool_name: &str, args: &serde_json::Value) -> bool {
    matches!(
        tool_name,
        "read_resource" | "write_resource" | "transfer_resource" | "open_app_surface" | "get_state"
    ) && args.get("target_id").is_some()
}

/// Identifies a stable resource only after its typed wire reference has been
/// parsed and checked against the operation it is allowed to perform.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AiStableResourceOperation {
    SavedConnection(crate::StableResourceRef),
    Settings,
    Rag,
    AppSurface(String),
}

/// Classifies the exact live-owner capability required by a v2 resource tool.
/// A resource family cannot be widened by selecting a handle of another kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiLiveResourceOperation {
    SftpRead,
    SftpWrite,
    SftpStartTransfer,
    IdeRead,
    IdeWrite,
}

impl AiLiveResourceOperation {
    pub const fn capability(self) -> crate::RuntimeCapability {
        match self {
            Self::SftpRead => crate::RuntimeCapability::SftpRead,
            Self::SftpWrite => crate::RuntimeCapability::SftpWrite,
            Self::SftpStartTransfer => crate::RuntimeCapability::SftpStartTransfer,
            Self::IdeRead => crate::RuntimeCapability::IdeRead,
            Self::IdeWrite => crate::RuntimeCapability::IdeWrite,
        }
    }

    pub const fn requires_ide_owner(self) -> bool {
        matches!(self, Self::IdeRead | Self::IdeWrite)
    }
}

pub fn ai_live_resource_operation(
    tool_name: &str,
    args: &serde_json::Value,
) -> Result<AiLiveResourceOperation, crate::RuntimeValidationError> {
    let resource = args.get("resource").and_then(serde_json::Value::as_str);
    let operation = match (tool_name, resource) {
        ("read_resource", Some("file" | "directory" | "sftp")) => AiLiveResourceOperation::SftpRead,
        ("read_resource", Some("ide")) => AiLiveResourceOperation::IdeRead,
        ("write_resource", Some("file")) => AiLiveResourceOperation::SftpWrite,
        ("write_resource", Some("ide")) => AiLiveResourceOperation::IdeWrite,
        ("transfer_resource", _) => AiLiveResourceOperation::SftpStartTransfer,
        _ => {
            return Err(crate::RuntimeValidationError::new(
                crate::RuntimeValidationFailure::CapabilityUnavailable,
            ));
        }
    };
    Ok(operation)
}

pub fn ai_stable_resource_operation(
    tool_name: &str,
    args: &serde_json::Value,
) -> Result<AiStableResourceOperation, crate::RuntimeValidationError> {
    let resource_ref = args
        .get("resource_ref")
        .cloned()
        .and_then(|value| serde_json::from_value::<crate::StableResourceRef>(value).ok())
        .ok_or_else(|| {
            crate::RuntimeValidationError::new(
                crate::RuntimeValidationFailure::CapabilityUnavailable,
            )
        })?;

    let operation = match tool_name {
        "connect_target" if resource_ref.kind() == crate::StableResourceKind::SavedConnection => {
            AiStableResourceOperation::SavedConnection(resource_ref)
        }
        "read_resource"
            if args.get("resource").and_then(serde_json::Value::as_str) == Some("settings")
                && resource_ref.kind() == crate::StableResourceKind::SettingsScope
                && resource_ref.id() == "app" =>
        {
            AiStableResourceOperation::Settings
        }
        "read_resource"
            if args.get("resource").and_then(serde_json::Value::as_str) == Some("rag")
                && resource_ref.kind() == crate::StableResourceKind::RagIndex
                && resource_ref.id() == "default" =>
        {
            AiStableResourceOperation::Rag
        }
        "write_resource"
            if args.get("resource").and_then(serde_json::Value::as_str) == Some("settings")
                && resource_ref.kind() == crate::StableResourceKind::SettingsScope
                && resource_ref.id() == "app" =>
        {
            AiStableResourceOperation::Settings
        }
        "open_app_surface" if resource_ref.kind() == crate::StableResourceKind::AppSurface => {
            AiStableResourceOperation::AppSurface(resource_ref.id().to_string())
        }
        _ => {
            return Err(crate::RuntimeValidationError::new(
                crate::RuntimeValidationFailure::CapabilityUnavailable,
            ));
        }
    };

    Ok(operation)
}

#[derive(Clone)]
pub struct AiActionResultLite {
    pub ok: bool,
    pub summary: String,
    pub output: String,
    pub data: serde_json::Value,
    pub images: Vec<crate::AiImageAttachment>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub risk: &'static str,
    pub target: Option<AiOrchestratorTarget>,
    pub targets: Vec<AiOrchestratorTarget>,
    pub next_actions: Vec<serde_json::Value>,
    pub observations: Vec<String>,
    pub verified: Option<bool>,
    pub state_version: Option<String>,
}

impl AiActionResultLite {
    pub fn with_target(mut self, target: AiOrchestratorTarget) -> Self {
        self.target = Some(target);
        self
    }

    pub fn with_targets(mut self, targets: Vec<AiOrchestratorTarget>) -> Self {
        self.targets = targets;
        self
    }

    pub fn with_next_actions(mut self, next_actions: Vec<serde_json::Value>) -> Self {
        self.next_actions = next_actions;
        self
    }

    pub fn with_verified(mut self, verified: bool) -> Self {
        self.verified = Some(verified);
        self
    }

    pub fn with_optional_target(mut self, target: Option<AiOrchestratorTarget>) -> Self {
        self.target = target;
        self
    }

    pub fn with_state_version(mut self, state_version: impl Into<String>) -> Self {
        self.state_version = Some(state_version.into());
        self
    }
}

pub fn ai_tool_verified_default(ok: bool, error_message: Option<&str>) -> bool {
    // Tauri marks an implicit result as verified only when it succeeded and did
    // not carry an error object.
    ok && error_message.is_none()
}

pub fn ai_run_command_preflight_risk() -> &'static str {
    // Tauri validates run_command target readiness and command text before the
    // terminal capability switches the action risk to interactive.
    "execute"
}

pub fn target_in_ai_view(target: &AiOrchestratorTarget, view: &str) -> bool {
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

pub fn normalized_ai_target_view(view: Option<&str>) -> &'static str {
    match view {
        Some("connections") => "connections",
        Some("live_sessions") => "live_sessions",
        Some("app_surfaces") => "app_surfaces",
        Some("files") => "files",
        Some("all") => "all",
        _ => "connections",
    }
}

pub fn target_matches_ai_query(target: &AiOrchestratorTarget, query: &str) -> bool {
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

pub fn normalized_ai_query(query: Option<&str>) -> String {
    // Tauri trims discovery queries before filtering targets.
    query.unwrap_or("").trim().to_lowercase()
}

pub fn view_for_ai_intent(intent: &str) -> &'static str {
    match intent {
        "command" | "terminal" => "live_sessions",
        "settings" | "app_surface" | "local" => "app_surfaces",
        "file" | "sftp" | "knowledge" => "files",
        _ => "connections",
    }
}

pub fn target_matches_active_context(
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

pub fn normalized_ai_intent(intent: Option<&str>) -> Option<&'static str> {
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

pub fn normalized_ai_select_target_kind(kind: Option<&str>) -> Option<&'static str> {
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

pub fn is_ai_command_like_query(query: &str) -> bool {
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

pub struct AiConnectionCounts {
    pub total: usize,
    pub saved: usize,
    pub live: usize,
    pub link_down: usize,
    pub error: usize,
}

pub fn ai_connection_counts(targets: &[AiOrchestratorTarget]) -> AiConnectionCounts {
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

pub fn risk_to_capability(risk: &str) -> Option<&'static str> {
    match risk {
        "read" => Some("state.list"),
        "write" => Some("filesystem.write"),
        "execute" => Some("command.run"),
        "interactive" => Some("terminal.send"),
        _ => None,
    }
}

pub fn ai_next_action_json(action: &serde_json::Value) -> Option<serde_json::Value> {
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

pub fn ai_target_projection(target: &AiOrchestratorTarget) -> crate::AiTargetProjection {
    // Never attach sampled terminal output to a target used for discovery.
    crate::AiTargetProjection {
        id: target.id.clone(),
        kind: target.kind.clone(),
        label: target.label.clone(),
        state: target.state.clone(),
        capabilities: target.capabilities.clone(),
        refs: target.refs.clone(),
        metadata: target.metadata.clone(),
    }
}

pub fn ai_target_from_projection(projection: crate::AiTargetProjection) -> AiOrchestratorTarget {
    AiOrchestratorTarget {
        id: projection.id,
        kind: projection.kind,
        label: projection.label,
        state: projection.state,
        capabilities: projection.capabilities,
        refs: projection.refs,
        metadata: projection.metadata,
        terminal_buffer: None,
        terminal_screen: None,
    }
}

pub fn ai_model_visible_settings_projection(
    settings: &oxideterm_settings::PersistedSettings,
) -> serde_json::Value {
    // Only explicitly approved, non-secret settings may cross the model boundary.
    serde_json::json!({
        "ai": {
            "enabled": settings.ai.enabled,
            "toolUse": {
                "enabled": settings.ai.tool_use.enabled,
                "maxRounds": settings.ai.tool_use.max_rounds,
                "maxCallsPerRound": settings.ai.tool_use.max_calls_per_round,
                "autoApproveTools": settings.ai.tool_use.auto_approve_tools,
                "disabledTools": settings.ai.tool_use.disabled_tools,
            },
        },
        "terminal": {
            "renderer": settings.terminal.renderer,
            "encoding": settings.terminal.terminal_encoding,
        },
        "sftp": {
            "directoryParallelism": settings.sftp.directory_parallelism,
            "transferProtocol": settings.sftp.transfer_protocol,
        }
    })
}

pub fn ai_runtime_validation_public_code(
    error: &crate::RuntimeValidationError,
    post_user_approval: bool,
) -> &'static str {
    if post_user_approval {
        "runtime_state_changed_after_approval"
    } else {
        error.public_code()
    }
}

pub fn ai_runtime_validation_recovery_message(post_user_approval: bool) -> &'static str {
    if post_user_approval {
        "Nothing was executed because the live target changed while approval was open. Rediscover it before retrying."
    } else {
        "Rediscover the current runtime target before retrying."
    }
}
