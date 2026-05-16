use std::collections::BTreeMap;

use sha2::Digest as _;

const AI_MAX_TOOL_ROUNDS_PER_REPLY: usize = 30;
const AI_MAX_REQUIRED_TOOL_RETRIES: usize = 1;
const AI_MAX_HARD_DENY_RETRIES: usize = 1;
const AI_PSEUDO_TOOL_RETRY_TOOL_NAME: &str = "tool_use_disabled";

#[derive(Clone)]
struct AiOrchestratorRuntimeSnapshot {
    targets: Vec<AiOrchestratorTarget>,
    memory: String,
    settings_summary: serde_json::Value,
    node_router: NodeRouter,
    sftp_transfer_manager: std::sync::Arc<SftpTransferManager>,
    agent_fs: NodeAgentIdeFileSystem,
    backend_runtime: std::sync::Arc<tokio::runtime::Runtime>,
    mcp_registry: oxideterm_ai::McpRegistry,
    rag_store: std::sync::Arc<oxideterm_ai::RagStore>,
    ai_key_store: oxideterm_ai::AiProviderKeyStore,
    ai_providers: Vec<serde_json::Value>,
    ai_embedding_config: Option<serde_json::Value>,
    ai_context_window: usize,
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
    ssh_handle: Option<SshConnectionHandle>,
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
    TrimNotice(usize),
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
