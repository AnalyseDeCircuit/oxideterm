//! JSON-RPC protocol types (backend mirror of agent/src/protocol.rs)
//!
//! These types match the agent's wire format exactly.
//! Requests are serialized as line-delimited JSON over the SSH exec channel.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

// ═══════════════════════════════════════════════════════════════════════════
// Request ID generator
// ═══════════════════════════════════════════════════════════════════════════

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Generate a unique request ID (monotonically increasing).
pub fn next_request_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

// ═══════════════════════════════════════════════════════════════════════════
// JSON-RPC envelope
// ═══════════════════════════════════════════════════════════════════════════

/// Outgoing request to the agent.
#[derive(Debug, Serialize)]
pub struct AgentRequest {
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "serde_json::Value::is_null")]
    pub params: serde_json::Value,
}

/// Incoming response from the agent.
#[derive(Debug, Deserialize)]
pub struct AgentResponse {
    pub id: u64,
    #[serde(default)]
    pub result: Option<serde_json::Value>,
    #[serde(default)]
    pub error: Option<AgentRpcError>,
}

/// JSON-RPC error object.
#[derive(Debug, Deserialize, Clone)]
pub struct AgentRpcError {
    pub code: i32,
    pub message: String,
}

impl std::fmt::Display for AgentRpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Agent RPC error {}: {}", self.code, self.message)
    }
}

/// Server-initiated notification (no `id`).
#[derive(Debug, Deserialize)]
pub struct AgentNotification {
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// Any line from the agent is either a Response or a Notification.
/// We distinguish by the presence of an `id` field.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum AgentMessage {
    Response(AgentResponse),
    Notification(AgentNotification),
}

// ═══════════════════════════════════════════════════════════════════════════
// Agent error codes (must match agent/src/protocol.rs)
// ═══════════════════════════════════════════════════════════════════════════

pub const ERR_METHOD_NOT_FOUND: i32 = -32601;
pub const ERR_INVALID_PARAMS: i32 = -32602;
pub const ERR_INTERNAL: i32 = -32603;
pub const ERR_IO: i32 = -1;
pub const ERR_NOT_FOUND: i32 = -2;
pub const ERR_PERMISSION: i32 = -3;
pub const ERR_ALREADY_EXISTS: i32 = -4;

// ═══════════════════════════════════════════════════════════════════════════
// fs/* result types (deserialized from agent responses)
// ═══════════════════════════════════════════════════════════════════════════

/// fs/readFile result
#[derive(Debug, Deserialize, Serialize)]
pub struct ReadFileResult {
    pub content: String,
    pub hash: String,
    pub size: u64,
    pub mtime: u64,
    /// Content encoding: "plain" or "zstd+base64".
    #[serde(default = "default_encoding")]
    pub encoding: String,
}

fn default_encoding() -> String {
    "plain".to_string()
}

/// fs/writeFile result
#[derive(Debug, Deserialize, Serialize)]
pub struct WriteFileResult {
    pub hash: String,
    pub size: u64,
    pub mtime: u64,
    pub atomic: bool,
}

/// fs/stat result
#[derive(Debug, Deserialize, Serialize)]
pub struct StatResult {
    pub exists: bool,
    pub file_type: Option<String>,
    pub size: Option<u64>,
    pub mtime: Option<u64>,
    pub permissions: Option<String>,
}

/// File entry (used by listDir/listTree)
#[derive(Debug, Deserialize, Serialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub file_type: String,
    pub size: u64,
    pub mtime: Option<u64>,
    pub permissions: Option<String>,
    pub children: Option<Vec<FileEntry>>,
}

/// fs/listTree result — entries + truncation metadata
#[derive(Debug, Deserialize, Serialize)]
pub struct ListTreeResult {
    pub entries: Vec<FileEntry>,
    /// True if max_entries was reached and results are incomplete.
    pub truncated: bool,
    /// Total number of entries scanned.
    pub total_scanned: u32,
}

/// search/grep match
#[derive(Debug, Deserialize, Serialize)]
pub struct GrepMatch {
    pub path: String,
    pub line: u32,
    pub column: u32,
    pub text: String,
}

/// git/status result
#[derive(Debug, Deserialize, Serialize)]
pub struct GitStatusResult {
    pub branch: String,
    pub files: Vec<GitFileEntry>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GitFileEntry {
    pub path: String,
    pub status: String,
}

/// sys/info result
#[derive(Debug, Deserialize, Serialize)]
pub struct SysInfoResult {
    pub version: String,
    pub arch: String,
    pub os: String,
    pub pid: u32,
}

/// watch/event notification
#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct WatchEvent {
    pub path: String,
    pub kind: String,
}

// ═══════════════════════════════════════════════════════════════════════════
// symbols/* types (mirror of agent/src/protocol.rs)
// ═══════════════════════════════════════════════════════════════════════════

/// Symbol kind classification.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SymbolKind {
    Function,
    Class,
    Struct,
    Interface,
    Enum,
    Trait,
    TypeAlias,
    Constant,
    Variable,
    Module,
    Method,
}

/// A single symbol definition.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SymbolInfo {
    pub name: String,
    pub kind: SymbolKind,
    pub path: String,
    pub line: u32,
    pub column: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
}

/// symbols/index result
#[derive(Debug, Deserialize, Serialize)]
pub struct SymbolIndexResult {
    pub symbols: Vec<SymbolInfo>,
    pub file_count: u32,
}

// ═══════════════════════════════════════════════════════════════════════════
// Agent status
// ═══════════════════════════════════════════════════════════════════════════

/// Agent status for frontend display.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AgentStatus {
    /// Agent not deployed to this host
    NotDeployed,
    /// Deploying agent binary
    Deploying,
    /// Agent running and ready
    Ready {
        version: String,
        arch: String,
        pid: u32,
    },
    /// Agent failed to start
    Failed { reason: String },
    /// Architecture not supported (no binary available)
    UnsupportedArch { arch: String },
}
