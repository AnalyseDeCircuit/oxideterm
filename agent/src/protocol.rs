//! JSON-RPC protocol types for OxideTerm Agent communication.
//!
//! Wire format: line-delimited JSON over stdin/stdout.
//! - Requests have `id` + `method` + optional `params`
//! - Responses have `id` + `result` or `error`
//! - Notifications have `method` + `params` but NO `id`

use serde::{Deserialize, Serialize};

// ═══════════════════════════════════════════════════════════════════════════
// JSON-RPC envelope
// ═══════════════════════════════════════════════════════════════════════════

/// Incoming request from OxideTerm client.
#[derive(Debug, Deserialize)]
pub struct Request {
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// Outgoing response to OxideTerm client.
#[derive(Debug, Serialize)]
pub struct Response {
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

/// JSON-RPC error object.
#[derive(Debug, Serialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

/// Server-initiated notification (no `id`).
#[derive(Debug, Serialize)]
pub struct Notification {
    pub method: String,
    pub params: serde_json::Value,
}

impl Response {
    pub fn ok(id: u64, result: serde_json::Value) -> Self {
        Self {
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: u64, code: i32, message: impl Into<String>) -> Self {
        Self {
            id,
            result: None,
            error: Some(RpcError {
                code,
                message: message.into(),
            }),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Error codes
// ═══════════════════════════════════════════════════════════════════════════

pub const ERR_INVALID_PARAMS: i32 = -32602;
pub const ERR_METHOD_NOT_FOUND: i32 = -32601;
pub const ERR_INTERNAL: i32 = -32603;
pub const ERR_IO: i32 = -1;
pub const ERR_NOT_FOUND: i32 = -2;
pub const ERR_PERMISSION: i32 = -3;
pub const ERR_ALREADY_EXISTS: i32 = -4;
pub const ERR_CONFLICT: i32 = -5;

// ═══════════════════════════════════════════════════════════════════════════
// fs/* params & results
// ═══════════════════════════════════════════════════════════════════════════

/// fs/readFile params
#[derive(Debug, Deserialize)]
pub struct ReadFileParams {
    pub path: String,
    /// Max file size in bytes (default: 10MB). Returns error if exceeded.
    #[serde(default = "default_max_size")]
    pub max_size: u64,
}

fn default_max_size() -> u64 {
    10 * 1024 * 1024
}

/// fs/readFile result
#[derive(Debug, Serialize)]
pub struct ReadFileResult {
    pub content: String,
    /// SHA-256 hex digest of the raw bytes.
    pub hash: String,
    pub size: u64,
    pub mtime: u64,
    /// Content encoding: "plain" or "zstd+base64".
    #[serde(default = "default_encoding")]
    pub encoding: String,
}

/// fs/writeFile params
#[derive(Debug, Deserialize)]
pub struct WriteFileParams {
    pub path: String,
    pub content: String,
    /// If provided, only write if remote hash matches (optimistic lock).
    #[serde(default)]
    pub expect_hash: Option<String>,
    /// Content encoding: "plain" (default) or "zstd+base64" (compressed).
    #[serde(default = "default_encoding")]
    pub encoding: String,
}

fn default_encoding() -> String {
    "plain".to_string()
}

/// fs/writeFile result
#[derive(Debug, Serialize)]
pub struct WriteFileResult {
    pub hash: String,
    pub size: u64,
    pub mtime: u64,
    /// Always true — agent uses POSIX atomic rename.
    pub atomic: bool,
}

/// fs/stat params
#[derive(Debug, Deserialize)]
pub struct StatParams {
    pub path: String,
}

/// fs/stat result
#[derive(Debug, Serialize)]
pub struct StatResult {
    pub exists: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_type: Option<String>, // "file", "directory", "symlink", "other"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mtime: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<String>, // octal, e.g. "755"
}

/// fs/listDir params
#[derive(Debug, Deserialize)]
pub struct ListDirParams {
    pub path: String,
}

/// fs/listTree params — recursive directory listing
#[derive(Debug, Deserialize)]
pub struct ListTreeParams {
    pub path: String,
    /// Maximum depth to recurse (default: 3).
    #[serde(default = "default_max_depth")]
    pub max_depth: u32,
    /// Maximum total entries to return (default: 5000).
    #[serde(default = "default_max_entries")]
    pub max_entries: u32,
}

fn default_max_depth() -> u32 {
    3
}

fn default_max_entries() -> u32 {
    5000
}

/// fs/listTree result — wraps entries with truncation metadata.
#[derive(Debug, Serialize)]
pub struct ListTreeResult {
    pub entries: Vec<FileEntry>,
    /// True if max_entries was reached and results are incomplete.
    pub truncated: bool,
    /// Total number of entries scanned (may exceed entries.len() if truncated).
    pub total_scanned: u32,
}

/// A single file/directory entry.
#[derive(Debug, Serialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub file_type: String,
    pub size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mtime: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<String>,
    /// Children entries (only for directories in listTree).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<FileEntry>>,
}

/// fs/mkdir params
#[derive(Debug, Deserialize)]
pub struct MkdirParams {
    pub path: String,
    /// Create parent directories if they don't exist (like mkdir -p).
    #[serde(default)]
    pub recursive: bool,
}

/// fs/remove params
#[derive(Debug, Deserialize)]
pub struct RemoveParams {
    pub path: String,
    /// Recursively remove directories.
    #[serde(default)]
    pub recursive: bool,
}

/// fs/rename params
#[derive(Debug, Deserialize)]
pub struct RenameParams {
    pub old_path: String,
    pub new_path: String,
}

/// fs/chmod params
#[derive(Debug, Deserialize)]
pub struct ChmodParams {
    pub path: String,
    /// Octal permission string, e.g. "755".
    pub mode: String,
}

// ═══════════════════════════════════════════════════════════════════════════
// symbols/* params & results
// ═══════════════════════════════════════════════════════════════════════════

/// Symbol kind (language-agnostic classification).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolInfo {
    pub name: String,
    pub kind: SymbolKind,
    pub path: String,
    pub line: u32,
    pub column: u32,
    /// Containing class/struct name (if applicable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
}

/// symbols/index params — scan a directory for all symbols.
#[derive(Debug, Deserialize)]
pub struct SymbolIndexParams {
    pub path: String,
    /// Maximum files to scan (default: 500).
    #[serde(default = "default_max_files")]
    pub max_files: u32,
}

fn default_max_files() -> u32 {
    500
}

/// symbols/index result
#[derive(Debug, Serialize)]
pub struct SymbolIndexResult {
    pub symbols: Vec<SymbolInfo>,
    pub file_count: u32,
}

/// symbols/complete params — autocomplete a symbol prefix.
#[derive(Debug, Deserialize)]
pub struct SymbolCompleteParams {
    pub prefix: String,
    /// Root path for context (must have been indexed first).
    pub path: String,
    /// Max results (default: 20).
    #[serde(default = "default_complete_limit")]
    pub limit: u32,
}

fn default_complete_limit() -> u32 {
    20
}

/// symbols/definitions params — find all definitions of a symbol.
#[derive(Debug, Deserialize)]
pub struct SymbolDefinitionsParams {
    pub name: String,
    /// Root path for context (must have been indexed first).
    pub path: String,
}

// ═══════════════════════════════════════════════════════════════════════════
// watch/* params & results
// ═══════════════════════════════════════════════════════════════════════════

/// watch/start params
#[derive(Debug, Deserialize)]
pub struct WatchStartParams {
    pub path: String,
    /// Glob patterns to ignore (e.g. ["node_modules", ".git"]).
    #[serde(default)]
    pub ignore: Vec<String>,
}

/// watch/stop params
#[derive(Debug, Deserialize)]
pub struct WatchStopParams {
    pub path: String,
}

/// watch/event notification params (server → client).
#[derive(Debug, Serialize)]
pub struct WatchEvent {
    pub path: String,
    /// "create", "modify", "delete", "rename"
    pub kind: String,
}

// ═══════════════════════════════════════════════════════════════════════════
// search/* params & results
// ═══════════════════════════════════════════════════════════════════════════

/// search/grep params
#[derive(Debug, Deserialize)]
pub struct GrepParams {
    pub pattern: String,
    pub path: String,
    #[serde(default)]
    #[allow(dead_code)] // Parsed from JSON; full regex engine pending
    pub is_regex: bool,
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default = "default_grep_max")]
    pub max_results: u32,
    /// Glob patterns to ignore.
    #[serde(default)]
    pub ignore: Vec<String>,
}

fn default_grep_max() -> u32 {
    500
}

#[derive(Debug, Serialize)]
pub struct GrepMatch {
    pub path: String,
    pub line: u32,
    pub column: u32,
    pub text: String,
}

// ═══════════════════════════════════════════════════════════════════════════
// git/* params & results
// ═══════════════════════════════════════════════════════════════════════════

/// git/status params
#[derive(Debug, Deserialize)]
pub struct GitStatusParams {
    pub path: String,
}

/// git/status result
#[derive(Debug, Serialize)]
pub struct GitStatusResult {
    pub branch: String,
    pub files: Vec<GitFileEntry>,
}

#[derive(Debug, Serialize)]
pub struct GitFileEntry {
    pub path: String,
    pub status: String, // "M", "A", "D", "?", "R", etc.
}

// ═══════════════════════════════════════════════════════════════════════════
// sys/* params & results
// ═══════════════════════════════════════════════════════════════════════════

/// sys/info result
#[derive(Debug, Serialize)]
pub struct SysInfoResult {
    pub version: String,
    pub arch: String,
    pub os: String,
    pub pid: u32,
    /// Supported capabilities: ["zstd"]
    #[serde(default)]
    pub capabilities: Vec<String>,
}
