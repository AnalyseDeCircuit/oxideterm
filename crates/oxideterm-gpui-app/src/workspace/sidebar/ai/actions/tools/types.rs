use std::collections::BTreeMap;

use sha2::Digest as _;

const AI_MAX_REQUIRED_TOOL_RETRIES: usize = 1;
const AI_MAX_HARD_DENY_RETRIES: usize = 1;
const AI_PSEUDO_TOOL_RETRY_TOOL_NAME: &str = "tool_use_disabled";

#[derive(Clone)]
struct AiOrchestratorRuntimeSnapshot {
    targets: Vec<AiOrchestratorTarget>,
    active_tab: Option<serde_json::Value>,
    active_node: Option<serde_json::Value>,
    active_session_id: Option<String>,
    active_tab_id: Option<String>,
    active_node_id: Option<String>,
    memory: serde_json::Value,
    health_state: serde_json::Value,
    settings_state: serde_json::Value,
    settings_summary: serde_json::Value,
    node_router: NodeRouter,
    sftp_transfer_manager: std::sync::Arc<SftpTransferManager>,
    agent_fs: NodeAgentIdeFileSystem,
    backend_runtime: std::sync::Arc<tokio::runtime::Runtime>,
    rag_store: std::sync::Arc<oxideterm_ai::RagStore>,
    ai_mcp_registry: oxideterm_ai::McpRegistry,
    ai_acp_runtime_registry: oxideterm_ai::AcpRuntimeRegistry,
    ai_key_store: oxideterm_ai::AiProviderKeyStore,
    ai_providers: Vec<serde_json::Value>,
    ai_embedding_config: Option<serde_json::Value>,
    ai_context_window: usize,
    runtime_epoch: String,
}

#[derive(Clone, Debug)]
struct AiOrchestratorTarget {
    id: String,
    kind: String,
    label: String,
    state: String,
    capabilities: Vec<String>,
    refs: BTreeMap<String, String>,
    metadata: serde_json::Value,
    terminal_buffer: Option<String>,
    terminal_screen: Option<serde_json::Value>,
}

#[derive(Clone, Debug)]
pub(super) struct AiExecutedToolResult {
    tool_call_id: String,
    tool_name: String,
    success: bool,
    output: String,
    error: Option<String>,
    duration_ms: u128,
    envelope: serde_json::Value,
}

#[derive(Debug)]
enum AiRemoteFileWriteError {
    ExpectedHashMismatch { expected: String, current: String },
    ExpectedFileMissing { path: String },
    ExistingFileNotText { path: String },
    Sftp(oxideterm_ssh::SftpError),
    Other(String),
}

pub(super) enum AiStreamDeliveryEvent {
    Stream(AiStreamEvent),
    AcpClientEvent(oxideterm_ai::AcpClientEvent),
    AcpSessionStarted {
        session_id: String,
        session_metadata: Option<serde_json::Value>,
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
    ToolExecutionRequested {
        tool_call_id: String,
        name: String,
        args: serde_json::Value,
        sender: tokio::sync::oneshot::Sender<AiExecutedToolResult>,
    },
}
