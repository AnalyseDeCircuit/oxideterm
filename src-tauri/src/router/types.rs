//! Oxide-Next 路由层类型定义
//!
//! 参考: docs/OXIDE_NEXT_ARCHITECTURE.md §3.1, §3.4

use serde::Serialize;

use crate::sftp::error::SftpError;
use crate::ssh::HandleController;

// ============================================================================
// Route Error
// ============================================================================

/// 路由错误类型
#[derive(Debug, thiserror::Error)]
pub enum RouteError {
    #[error("Node not found: {0}")]
    NodeNotFound(String),

    #[error("No active connection for node: {0}")]
    NotConnected(String),

    #[error("Connection in error state: {0}")]
    ConnectionError(String),

    #[error("Capability unavailable: {0}")]
    CapabilityUnavailable(String),

    #[error("{0}")]
    SftpOperationError(String),

    #[error("Connection timeout: {0}")]
    ConnectionTimeout(String),
}

impl From<SftpError> for RouteError {
    fn from(e: SftpError) -> Self {
        match &e {
            SftpError::SessionNotFound(_) | SftpError::NotInitialized(_) => {
                RouteError::CapabilityUnavailable(e.to_string())
            }
            _ => RouteError::SftpOperationError(e.to_string()),
        }
    }
}

impl Serialize for RouteError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

// ============================================================================
// Resolved Connection
// ============================================================================

/// resolve_connection 的返回值：已解析的连接信息
pub struct ResolvedConnection {
    pub connection_id: String,
    pub handle_controller: HandleController,
    pub terminal_session_id: Option<String>,
    pub sftp_session_id: Option<String>,
}

// ============================================================================
// Terminal Endpoint
// ============================================================================

/// 终端 WebSocket 端点信息
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalEndpoint {
    pub ws_port: u16,
    pub ws_token: String,
    pub session_id: String,
}

// ============================================================================
// Node State (前端消费)
// ============================================================================

/// 节点就绪状态（前端唯一需要关心的状态）
///
/// 序列化为 snake_case 字符串: "ready" | "connecting" | "error" | "disconnected"
/// 错误详情通过 NodeState.error 或 NodeStateEvent.reason 传递。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeReadiness {
    /// 完全就绪，可执行所有操作
    Ready,
    /// 正在连接/重连中
    Connecting,
    /// 连接错误（详情见 NodeState.error）
    Error,
    /// 已断开
    Disconnected,
}

/// 节点完整状态（useNodeState 消费）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeState {
    pub readiness: NodeReadiness,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub sftp_ready: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sftp_cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ws_endpoint: Option<TerminalEndpoint>,
}

/// node_get_state 返回值：状态 + 当前 generation（v1.2 快照对齐）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeStateSnapshot {
    pub state: NodeState,
    pub generation: u64,
}

// ============================================================================
// Node State Event (后端推送)
// ============================================================================

/// 后端状态变更事件（取代 refreshConnections 轮询）
///
/// 有序性保证：每个事件携带 generation（每节点单调递增计数器），
/// 前端必须丢弃 generation <= 已见最大值的事件。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum NodeStateEvent {
    /// 连接状态变更
    ConnectionStateChanged {
        node_id: String,
        generation: u64,
        state: NodeReadiness,
        reason: String,
    },
    /// SFTP 就绪状态变更
    SftpReady {
        node_id: String,
        generation: u64,
        ready: bool,
        cwd: Option<String>,
    },
    /// 终端 WebSocket 信息变更（重连后 URL 可能变化）
    TerminalEndpointChanged {
        node_id: String,
        generation: u64,
        ws_port: u16,
        ws_token: String,
    },
}
