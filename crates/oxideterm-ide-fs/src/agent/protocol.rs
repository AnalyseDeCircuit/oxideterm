#[derive(Debug, Serialize)]
struct AgentRequest {
    id: u64,
    method: String,
    #[serde(skip_serializing_if = "serde_json::Value::is_null")]
    params: serde_json::Value,
}

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Deserialize)]
struct AgentNotification {
    method: String,
    #[serde(default)]
    params: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum AgentMessage {
    Response(AgentResponse),
    Notification(AgentNotification),
}

#[derive(Debug, Deserialize, Serialize)]
struct ReadFileResult {
    content: String,
    hash: String,
    size: u64,
    mtime: u64,
    #[serde(default = "plain_encoding")]
    encoding: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct WriteFileResult {
    hash: String,
    size: u64,
    mtime: u64,
    atomic: bool,
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
