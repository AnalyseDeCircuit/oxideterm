use std::{
    collections::{HashMap, HashSet},
    fmt,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

use base64::Engine as _;
use futures_util::{FutureExt as _, StreamExt as _, future::BoxFuture};
use parking_lot::RwLock;
use reqwest::{StatusCode, header::HeaderName};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    process::{Child, Command},
    sync::{Mutex, broadcast, oneshot},
    task::JoinHandle,
};
use zeroize::Zeroizing;

use crate::{AiProviderKeyStore, AiToolDefinition};

const MCP_CLIENT_NAME: &str = "OxideTerm";
const MCP_CLIENT_VERSION: &str = "1.0.0";
const MAX_MCP_MESSAGE_BYTES: usize = 10 * 1024 * 1024;
const MCP_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MCP_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(3);
const MCP_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3);
const MCP_TOOL_OUTPUT_MAX_CHARS: usize = 8_192;
const MCP_MAX_RETRIES: u32 = 3;
const MCP_RETRY_BASE_DELAY: Duration = Duration::from_secs(1);
const MCP_MAX_MULTI_ROUND_TRIPS: usize = 16;
const MCP_TASK_MAX_WAIT: Duration = Duration::from_secs(15 * 60);
const MCP_TASK_DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(1);
const MCP_TASK_MIN_POLL_INTERVAL: Duration = Duration::from_millis(100);
const MCP_TASK_MAX_POLL_INTERVAL: Duration = Duration::from_secs(30);

type PendingMap = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, McpError>>>>>;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum McpTransport {
    Stdio,
    StreamableHttp,
    LegacySse,
    Sse,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum McpEffectiveTransport {
    Stdio,
    StreamableHttp,
    LegacySse,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum McpAuthHeaderMode {
    Bearer,
    Raw,
    None,
}

#[derive(Clone, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    pub id: String,
    pub name: String,
    pub transport: McpTransport,
    pub url: Option<String>,
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    pub auth_header_name: Option<String>,
    pub auth_header_mode: Option<McpAuthHeaderMode>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub retry_on_disconnect: bool,
    #[serde(default)]
    pub auth_token: Option<String>,
}

impl fmt::Debug for McpServerConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("McpServerConfig")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("transport", &self.transport)
            .field("url", &self.url)
            .field("command", &self.command)
            .field("args", &redact_sensitive_args(&self.args))
            .field("env", &redacted_map_debug(&self.env))
            .field("auth_header_name", &self.auth_header_name)
            .field("auth_header_mode", &self.auth_header_mode)
            .field("headers", &redacted_map_debug(&self.headers))
            .field("enabled", &self.enabled)
            .field("retry_on_disconnect", &self.retry_on_disconnect)
            .field(
                "auth_token",
                &self.auth_token.as_ref().map(|_| "[redacted token]"),
            )
            .finish()
    }
}

fn redacted_map_debug(map: &HashMap<String, String>) -> HashMap<&str, &'static str> {
    map.keys()
        .map(|key| (key.as_str(), "[redacted]"))
        .collect::<HashMap<_, _>>()
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerCapabilities {
    #[serde(default)]
    pub tools: Option<Value>,
    #[serde(default)]
    pub resources: Option<Value>,
    #[serde(default)]
    pub prompts: Option<Value>,
    #[serde(default)]
    pub extensions: Option<HashMap<String, Value>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolSchema {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Value,
    #[serde(default)]
    pub output_schema: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpResource {
    pub uri: String,
    pub name: String,
    pub description: Option<String>,
    pub mime_type: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpResourceContent {
    pub uri: String,
    pub mime_type: Option<String>,
    pub text: Option<String>,
    pub blob: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct McpCallContent {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: Option<String>,
    pub data: Option<String>,
    pub mime_type: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpCallToolResult {
    #[serde(default)]
    pub content: Vec<McpCallContent>,
    #[serde(default)]
    pub is_error: bool,
    #[serde(default)]
    pub structured_content: Option<Value>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerStateSnapshot {
    pub config: McpServerConfig,
    pub status: &'static str,
    pub error: Option<String>,
    pub capabilities: Option<McpServerCapabilities>,
    pub tools: Vec<McpToolSchema>,
    pub resources: Vec<McpResource>,
    pub runtime_id: Option<String>,
    pub endpoint_url: Option<String>,
    pub resolved_transport: Option<String>,
    pub session_id: Option<String>,
    pub protocol_era: Option<String>,
    pub protocol_version: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum McpError {
    #[error("{0}")]
    Message(String),
    #[error("MCP HTTP request failed: {0} {1}")]
    HttpStatus(StatusCode, String),
    #[error("MCP error {code}: {message}")]
    Rpc {
        code: i64,
        message: String,
        data: Option<Value>,
        status: Option<StatusCode>,
    },
    #[error("MCP server {0} timed out (30s)")]
    Timeout(String),
    #[error("MCP server {0} is not connected")]
    NotConnected(String),
}

impl McpError {
    fn is_connection_failure(&self) -> bool {
        match self {
            Self::Timeout(_) | Self::NotConnected(_) => true,
            Self::HttpStatus(status, _) => status.is_server_error(),
            Self::Message(message) => {
                message.contains("connection lost")
                    || message.contains("closed stdout")
                    || message.contains("MCP server closed")
                    || message.contains("Failed to write to MCP server")
            }
            Self::Rpc { status, .. } => status.is_some_and(|status| status.is_server_error()),
        }
    }
}

struct McpProcess {
    child: Mutex<Child>,
    stdin: Mutex<tokio::process::ChildStdin>,
    next_id: AtomicU64,
    pending: PendingMap,
    notifications: broadcast::Sender<Value>,
    reader_task: JoinHandle<()>,
    stderr_task: JoinHandle<()>,
    eof_shutdown: AtomicBool,
}

#[derive(Default)]
struct McpProcessRegistry {
    processes: Mutex<HashMap<String, Arc<McpProcess>>>,
    #[cfg(test)]
    stop_all_calls: std::sync::atomic::AtomicUsize,
}

/// Owns the shared process registry until every registry handle and process task is gone.
#[derive(Default)]
struct McpProcessOwner {
    registry: Arc<McpProcessRegistry>,
}

impl std::ops::Deref for McpProcessOwner {
    type Target = McpProcessRegistry;

    fn deref(&self) -> &Self::Target {
        &self.registry
    }
}

#[derive(Clone)]
struct McpServerState {
    config: McpServerConfig,
    status: McpServerStatus,
    error: Option<String>,
    capabilities: Option<McpServerCapabilities>,
    tools: Vec<McpToolSchema>,
    resources: Vec<McpResource>,
    runtime_id: Option<String>,
    endpoint_url: Option<String>,
    resolved_transport: Option<McpEffectiveTransport>,
    session_id: Option<String>,
    protocol: Option<McpProtocol>,
    tools_cache: Option<McpCachedResult<Vec<McpToolSchema>>>,
    resources_cache: Option<McpCachedResult<Vec<McpResource>>>,
    resource_content_cache: HashMap<String, McpCachedResult<Vec<McpResourceContent>>>,
    resource_subscriptions: HashSet<String>,
    subscription_abort: Option<tokio::task::AbortHandle>,
    generation: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum McpServerStatus {
    Disconnected,
    Connecting,
    Connected,
    Error,
}

impl McpServerStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Disconnected => "disconnected",
            Self::Connecting => "connecting",
            Self::Connected => "connected",
            Self::Error => "error",
        }
    }
}

impl McpTransport {
    fn effective(self) -> McpEffectiveTransport {
        match self {
            Self::Stdio => McpEffectiveTransport::Stdio,
            Self::StreamableHttp | Self::Sse => McpEffectiveTransport::StreamableHttp,
            Self::LegacySse => McpEffectiveTransport::LegacySse,
        }
    }
}

impl McpEffectiveTransport {
    fn as_str(self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::StreamableHttp => "streamable-http",
            Self::LegacySse => "legacy-sse",
        }
    }
}

impl McpServerState {
    fn disconnected(config: McpServerConfig, generation: u64) -> Self {
        Self {
            config,
            status: McpServerStatus::Disconnected,
            error: None,
            capabilities: None,
            tools: Vec::new(),
            resources: Vec::new(),
            runtime_id: None,
            endpoint_url: None,
            resolved_transport: None,
            session_id: None,
            protocol: None,
            tools_cache: None,
            resources_cache: None,
            resource_content_cache: HashMap::new(),
            resource_subscriptions: HashSet::new(),
            subscription_abort: None,
            generation,
        }
    }

    fn snapshot(&self) -> McpServerStateSnapshot {
        McpServerStateSnapshot {
            config: redacted_mcp_config(&self.config),
            status: self.status.as_str(),
            error: self.error.clone(),
            capabilities: self.capabilities.clone(),
            tools: self.tools.clone(),
            resources: self.resources.clone(),
            runtime_id: self.runtime_id.clone(),
            endpoint_url: self.endpoint_url.clone(),
            resolved_transport: self
                .resolved_transport
                .map(|transport| transport.as_str().to_string()),
            session_id: self.session_id.clone(),
            protocol_era: self
                .protocol
                .as_ref()
                .map(|protocol| protocol.era.as_str().to_string()),
            protocol_version: self
                .protocol
                .as_ref()
                .map(|protocol| protocol.version.clone()),
        }
    }
}

#[derive(Default)]
struct McpRuntimeState {
    servers: HashMap<String, McpServerState>,
    server_order: Vec<String>,
    tool_index: HashMap<String, (String, String)>,
    generations: HashMap<String, u64>,
    retry_counters: HashMap<String, u32>,
}

#[derive(Clone)]
pub struct McpRegistry {
    state: Arc<RwLock<McpRuntimeState>>,
    processes: Arc<McpProcessOwner>,
    key_store: AiProviderKeyStore,
}
