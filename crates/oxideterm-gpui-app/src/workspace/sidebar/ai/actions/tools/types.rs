use std::collections::BTreeMap;

pub(in crate::workspace) const AI_MAX_HARD_DENY_RETRIES: usize = 1;
pub(in crate::workspace) const AI_PSEUDO_TOOL_RETRY_TOOL_NAME: &str = "tool_use_disabled";
pub(in crate::workspace) const AI_RUNTIME_STABLE_RESOURCE_LIMIT: usize = 128;
pub(in crate::workspace) const AI_TARGET_DISCOVERY_LIMIT: usize = 128;

pub(in crate::workspace) struct AiOrchestratorRuntimeSnapshot {
    pub(in crate::workspace) targets: Vec<AiOrchestratorTarget>,
    /// Maps internal target identities to the current tool session's opaque handles.
    pub(in crate::workspace) runtime_handles: HashMap<String, oxideterm_ai::RuntimeHandleProjection>,
    pub(in crate::workspace) active_tab: Option<serde_json::Value>,
    pub(in crate::workspace) active_node: Option<serde_json::Value>,
    pub(in crate::workspace) active_session_id: Option<String>,
    pub(in crate::workspace) active_tab_id: Option<String>,
    pub(in crate::workspace) active_node_id: Option<String>,
    pub(in crate::workspace) memory: serde_json::Value,
    pub(in crate::workspace) health_state: serde_json::Value,
    pub(in crate::workspace) transfers_state: serde_json::Value,
    pub(in crate::workspace) model_visible_settings: serde_json::Value,
}

use crate::workspace::ai_state::agents::AiModelBackendServices;

/// Concrete application adapters used only after the GPUI broker validates a
/// live capability handle. This type must never enter a provider task.
#[derive(Clone)]
pub(in crate::workspace) struct AiLiveToolServices {
    pub(in crate::workspace) node_router: NodeRouter,
    pub(in crate::workspace) sftp_transfer_manager: std::sync::Arc<SftpTransferManager>,
    pub(in crate::workspace) backend_runtime: std::sync::Arc<tokio::runtime::Runtime>,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::workspace) struct AiModelRuntimeState {
    pub(in crate::workspace) context_window: usize,
}

pub(in crate::workspace) struct AiAcpChatLaunch {
    pub(in crate::workspace) launch_config: oxideterm_ai::AcpLaunchConfig,
    pub(in crate::workspace) session_cwd: std::path::PathBuf,
    pub(in crate::workspace) host_policy: oxideterm_ai::AcpHostCapabilityPolicy,
}

#[derive(Clone, Debug)]
pub(in crate::workspace) struct AiOrchestratorTarget {
    pub(in crate::workspace) id: String,
    pub(in crate::workspace) kind: String,
    pub(in crate::workspace) label: String,
    pub(in crate::workspace) state: String,
    pub(in crate::workspace) capabilities: Vec<String>,
    pub(in crate::workspace) refs: BTreeMap<String, String>,
    pub(in crate::workspace) metadata: serde_json::Value,
    pub(in crate::workspace) terminal_buffer: Option<String>,
    pub(in crate::workspace) terminal_screen: Option<serde_json::Value>,
}

pub(in crate::workspace) enum AiRemoteFileWriteError {
    OwnerReplaced,
    ExpectedHashMismatch,
    ExpectedFileMissing { path: String },
    ExistingFileNotText { path: String },
    Sftp(oxideterm_ssh::SftpError),
}

impl std::fmt::Debug for AiRemoteFileWriteError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Paths, content hashes, and transport details are deliberately absent
        // because this error can cross task and diagnostic boundaries.
        let variant = match self {
            Self::OwnerReplaced => "OwnerReplaced",
            Self::ExpectedHashMismatch => "ExpectedHashMismatch",
            Self::ExpectedFileMissing { .. } => "ExpectedFileMissing",
            Self::ExistingFileNotText { .. } => "ExistingFileNotText",
            Self::Sftp(_) => "Sftp",
        };
        formatter
            .debug_struct("AiRemoteFileWriteError")
            .field("variant", &variant)
            .finish()
    }
}

/// Separates a stale SFTP authority from an operation failure without retaining
/// raw transfer arguments in an error's debug representation.
pub(in crate::workspace) enum AiSftpTransferError {
    OwnerReplaced,
    Operation(String),
}

pub(in crate::workspace) enum AiStreamDeliveryEvent {
    Checkpoint(oxideterm_ai::agent::AgentCheckpoint),
    ToolResourcesRequested {
        tool_session_id: ToolSessionId,
        name: String,
        args: serde_json::Value,
        sender: tokio::sync::oneshot::Sender<Result<Vec<oxideterm_ai::RuntimeOwnerKey>, String>>,
    },
    AgentCommandRequested {
        dispatch: Option<oxideterm_ai::agent::AgentDispatch>,
        tool_session_id: ToolSessionId,
        call: AiToolCall,
        sender: tokio::sync::oneshot::Sender<AiExecutedToolResult>,
    },
    Stream(AiStreamEvent),
    PromptUsage {
        last_user_message_id: Option<String>,
        provider_id: String,
        model: String,
        breakdown: oxideterm_ai::AiPromptTokenBreakdown,
        max_tokens: usize,
    },
    AcpClientEvent {
        agent_id: String,
        event: oxideterm_ai::AcpClientEvent,
    },
    AcpSessionStarted {
        session_id: String,
        session_metadata: Option<serde_json::Value>,
        session_config_options: Vec<oxideterm_ai::AcpSessionConfigOption>,
        session_modes: Option<oxideterm_ai::AcpSessionModeState>,
        agent_id: String,
    },
    Guardrail {
        code: String,
        message: String,
        raw_text: Option<String>,
    },
    AssistantRound {
        round_id: String,
        round_number: i64,
        response_length: usize,
        tool_call_ids: Vec<String>,
        synthetic: bool,
        retry_attempt: Option<usize>,
        hard_deny_triggered: bool,
    },
    RoundSummary {
        round_id: String,
        text: String,
        metadata: serde_json::Value,
    },
    RoundStatefulMarker {
        round_id: String,
        marker: Option<String>,
    },
    Diagnostic {
        event_type: String,
        round_id: Option<String>,
        data: serde_json::Value,
    },
    ToolStatus {
        tool_call_id: String,
        name: String,
        arguments: String,
        status: String,
        result: Option<serde_json::Value>,
        risk: Option<String>,
        summary: Option<String>,
        synthetic_denied: bool,
        raw_text: Option<String>,
        round_id: Option<String>,
        round_number: Option<i64>,
    },
    ToolApprovalRequested {
        tool_call_id: String,
        name: String,
        arguments: String,
        risk: String,
        summary: String,
        sender: tokio::sync::oneshot::Sender<bool>,
    },
    /// Pauses one discovery call for an explicit human choice. The worker
    /// receives only the selected array index; live handles stay off the UI.
    ToolCandidateSelectionRequested {
        tool_call_id: String,
        name: String,
        arguments: String,
        candidates: Vec<serde_json::Value>,
        sender: tokio::sync::oneshot::Sender<Option<usize>>,
    },
    /// Requests a current-owner validation before a policy prompt is shown.
    /// The execution request repeats the same validation after approval.
    ToolPreflightRequested {
        tool_session_id: ToolSessionId,
        tool_call_id: String,
        name: String,
        args: serde_json::Value,
        sender: tokio::sync::oneshot::Sender<Option<AiExecutedToolResult>>,
    },
    /// Builds a fresh, data-only v2 projection before a provider round.
    RuntimeContextRequested {
        tool_session_id: ToolSessionId,
        sender: tokio::sync::oneshot::Sender<Option<String>>,
    },
    ToolExecutionRequested {
        dispatch: Option<oxideterm_ai::agent::AgentDispatch>,
        leases: Vec<oxideterm_ai::agent::AgentToolLease>,
        tool_session_id: ToolSessionId,
        tool_call_id: String,
        name: String,
        args: serde_json::Value,
        post_user_approval: bool,
        dangerous_command_approved: bool,
        sender: tokio::sync::oneshot::Sender<AiExecutedToolResult>,
    },
}

#[derive(Clone)]
pub(in crate::workspace) struct AiToolRunContext {
    arguments: zeroize::Zeroizing<String>,
    generation: u64,
    conversation_id: String,
    assistant_id: String,
    dispatch: Option<oxideterm_ai::agent::AgentDispatch>,
}

impl AiToolRunContext {
    fn resource(&self, workspace: &WorkspaceApp, cx: &App, kind: oxideterm_ai::agent::OwnedResourceKind,
        label: &str) -> Option<oxideterm_ai::agent::AgentResourceRecord> {
        let ai = workspace.ai_entity.read(cx);
        let run = ai.agent_run(self.generation)?;
        ai.agents.services.runtime.register_resource(&run, kind, AgentText::new(label)).ok()
    }
}
