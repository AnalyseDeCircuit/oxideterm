#[derive(Serialize)]
struct AgentRequest {
    id: u64,
    method: String,
    #[serde(skip_serializing_if = "serde_json::Value::is_null")]
    params: serde_json::Value,
}

#[derive(Deserialize)]
struct AgentResponse {
    id: u64,
    #[serde(default)]
    result: Option<serde_json::Value>,
    #[serde(default)]
    error: Option<AgentRpcError>,
}

#[derive(Clone, Debug, Deserialize)]
struct AgentRpcError {
    code: i32,
    message: String,
}

#[derive(Deserialize)]
struct AgentNotification {
    method: String,
    #[serde(default)]
    params: serde_json::Value,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum AgentMessage {
    Response(AgentResponse),
    Notification(AgentNotification),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ReadFileResult {
    pub content: String,
    pub hash: String,
    pub size: u64,
    pub mtime: u64,
    #[serde(default = "plain_encoding")]
    pub encoding: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WriteFileResult {
    pub hash: String,
    pub size: u64,
    pub mtime: u64,
    pub atomic: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum NodeAgentRpcError {
    #[error("Agent unavailable: {0}")]
    Unavailable(String),
    #[error("Agent conflict: {0}")]
    Conflict(String),
    #[error("Agent RPC failed: {0}")]
    Other(String),
}

#[derive(Debug, Deserialize, Serialize)]
struct StatResult {
    exists: bool,
    file_type: Option<String>,
    size: Option<u64>,
    mtime: Option<u64>,
    permissions: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct FileEntry {
    name: String,
    path: String,
    file_type: String,
    #[serde(default)]
    is_symlink: bool,
    symlink_target: Option<String>,
    target_file_type: Option<String>,
    size: u64,
    mtime: Option<u64>,
    permissions: Option<String>,
    children: Option<Vec<FileEntry>>,
    #[serde(default)]
    truncated: bool,
}

#[derive(Debug, Deserialize, Serialize)]
struct SysInfoResult {
    version: String,
    #[serde(default = "legacy_agent_compatibility")]
    compatibility_version: u32,
    arch: String,
    os: String,
    pid: u32,
    #[serde(default)]
    capabilities: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AgentWatchEvent {
    pub path: String,
    pub kind: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct AgentGrepMatch {
    pub path: String,
    pub line: u32,
    pub column: u32,
    pub text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdeSearchMatch {
    pub path: String,
    pub line: u32,
    pub column: u32,
    pub preview: String,
    pub match_start: usize,
    pub match_end: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RemoteAgentVersionInfo {
    version: String,
    compatibility_version: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum RemoteAgentInstallState {
    Missing,
    Current,
    Incompatible(RemoteAgentVersionInfo),
}

fn plain_encoding() -> String {
    "plain".to_string()
}

fn legacy_agent_compatibility() -> u32 {
    LEGACY_AGENT_COMPATIBILITY_VERSION
}

type PendingMap =
    Arc<Mutex<HashMap<u64, oneshot::Sender<Result<serde_json::Value, AgentRpcError>>>>>;
