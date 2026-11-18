use oxideterm_ai::{
    AiExecutedToolResult, AiStreamEvent, AiToolCall, ToolSessionId, stream_state::AiCompactionPlan,
};

use crate::state::AiPendingChatStream;

pub struct AiStreamDelivery {
    pub generation: u64,
    pub conversation_id: String,
    pub assistant_id: String,
    pub event: AiStreamDeliveryEvent,
}

pub type AiStreamDeliverySender =
    oxideterm_workspace::delivery::ActiveDeliverySender<AiStreamDelivery>;

pub struct AiCompactionDelivery {
    pub generation: u64,
    pub kind: AiCompactionDeliveryKind,
    pub conversation_id: String,
    pub base_ids: Vec<String>,
    pub plan: Option<AiCompactionPlan>,
    pub summary: String,
    pub failed: bool,
    pub resume_after: Option<AiPendingChatStream>,
    pub silent: bool,
}

pub type AiCompactionDeliverySender =
    oxideterm_workspace::delivery::ActiveDeliverySender<AiCompactionDelivery>;

pub enum AiCompactionDeliveryKind {
    Compact,
    Summary,
}

pub enum AiStreamDeliveryEvent {
    ToolImages(Vec<oxideterm_ai::AiImageAttachment>),
    Checkpoint(oxideterm_ai::agent::AgentCheckpoint),
    HistoryBarrier(tokio::sync::oneshot::Sender<bool>),
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
    UserQuestionRequested {
        call: AiToolCall,
        dispatch: Option<oxideterm_ai::agent::AgentDispatch>,
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
    ToolExecutionRequested {
        audit_context: Option<oxideterm_audit::AuditContext>,
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

mod batch;
pub use batch::{AiChatDeliveryBatch, AiStreamTextKind, ai_context_token_breakdown_from_prompt};

/// Published after the window adapter has applied and acknowledged a chat batch.
pub struct AiChatBatchApplied;

impl gpui::EventEmitter<AiChatBatchApplied> for crate::state::AiWorkspaceEntity {}
