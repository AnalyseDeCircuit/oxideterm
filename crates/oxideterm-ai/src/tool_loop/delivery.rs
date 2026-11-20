use crate::{AiExecutedToolResult, AiStreamEvent, AiToolCall, ToolSessionId};

pub struct AiStreamDelivery {
    pub generation: u64,
    pub conversation_id: String,
    pub assistant_id: String,
    pub event: AiStreamDeliveryEvent,
}

pub type AiStreamDeliverySender =
    oxideterm_workspace::delivery::ActiveDeliverySender<AiStreamDelivery>;

pub enum AiStreamDeliveryEvent {
    ToolImages(Vec<crate::AiImageAttachment>),
    Checkpoint(crate::agent::AgentCheckpoint),
    HistoryBarrier(tokio::sync::oneshot::Sender<bool>),
    ToolResourcesRequested {
        tool_session_id: ToolSessionId,
        name: String,
        args: serde_json::Value,
        sender: tokio::sync::oneshot::Sender<Result<Vec<crate::RuntimeOwnerKey>, String>>,
    },
    AgentCommandRequested {
        dispatch: Option<crate::agent::AgentDispatch>,
        tool_session_id: ToolSessionId,
        call: AiToolCall,
        sender: tokio::sync::oneshot::Sender<AiExecutedToolResult>,
    },
    Stream(AiStreamEvent),
    PromptUsage {
        last_user_message_id: Option<String>,
        provider_id: String,
        model: String,
        breakdown: crate::AiPromptTokenBreakdown,
        max_tokens: usize,
    },
    AcpClientEvent {
        agent_id: String,
        event: crate::AcpClientEvent,
    },
    AcpSessionStarted {
        session_id: String,
        session_metadata: Option<serde_json::Value>,
        session_config_options: Vec<crate::AcpSessionConfigOption>,
        session_modes: Option<crate::AcpSessionModeState>,
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
    UserQuestionRequested {
        call: AiToolCall,
        dispatch: Option<crate::agent::AgentDispatch>,
        sender: tokio::sync::oneshot::Sender<zeroize::Zeroizing<String>>,
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
    /// Completed sanitized wire messages for one assistant turn (process memory only,
    /// never persisted) so subsequent turns reuse the provider prefix cache across tool rounds.
    TurnWireReplay {
        scope: u64,
        user_override: Option<(String, String)>,
        messages: Vec<crate::AiChatMessage>,
    },
    ToolExecutionRequested {
        audit_context: Option<oxideterm_audit::AuditContext>,
        dispatch: Option<crate::agent::AgentDispatch>,
        leases: Vec<crate::agent::AgentToolLease>,
        tool_session_id: ToolSessionId,
        tool_call_id: String,
        name: String,
        args: serde_json::Value,
        post_user_approval: bool,
        dangerous_command_approved: bool,
        sender: tokio::sync::oneshot::Sender<AiExecutedToolResult>,
    },
}
