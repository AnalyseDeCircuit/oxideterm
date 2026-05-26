use std::{
    collections::HashMap,
    fmt,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use futures_util::{FutureExt as _, StreamExt as _, future::BoxFuture};
use parking_lot::RwLock;
use reqwest::{Client, StatusCode, header::HeaderName};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    process::{Child, Command},
    sync::{Mutex, oneshot},
    task::JoinHandle,
};
use zeroize::Zeroizing;

use crate::{AiProviderKeyStore, AiToolDefinition};

const STREAMABLE_HTTP_PROTOCOL_VERSION: &str = "2025-11-25";
const LEGACY_SSE_PROTOCOL_VERSION: &str = "2024-11-05";
const MCP_CLIENT_NAME: &str = "OxideTerm";
const MCP_CLIENT_VERSION: &str = "1.0.0";
const MAX_MCP_MESSAGE_BYTES: usize = 10 * 1024 * 1024;
const MCP_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MCP_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3);
const MCP_TOOL_OUTPUT_MAX_CHARS: usize = 8_192;
const MCP_MAX_RETRIES: u32 = 3;
const MCP_RETRY_BASE_DELAY: Duration = Duration::from_secs(1);

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
            .field("auth_token", &self.auth_token.as_ref().map(|_| "[redacted token]"))
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
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolSchema {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Value,
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
}

#[derive(Debug, thiserror::Error)]
pub enum McpError {
    #[error("{0}")]
    Message(String),
    #[error("MCP HTTP request failed: {0} {1}")]
    HttpStatus(StatusCode, String),
    #[error("MCP server {0} timed out (30s)")]
    Timeout(String),
    #[error("MCP server {0} is not connected")]
    NotConnected(String),
}

struct McpProcess {
    child: Mutex<Child>,
    stdin: Mutex<tokio::process::ChildStdin>,
    next_id: AtomicU64,
    pending: PendingMap,
    reader_task: JoinHandle<()>,
    stderr_task: JoinHandle<()>,
}

#[derive(Default)]
struct McpProcessRegistry {
    processes: Mutex<HashMap<String, Arc<McpProcess>>>,
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
    fn protocol_version(self) -> &'static str {
        match self {
            Self::LegacySse => LEGACY_SSE_PROTOCOL_VERSION,
            Self::Stdio | Self::StreamableHttp => STREAMABLE_HTTP_PROTOCOL_VERSION,
        }
    }

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
    processes: Arc<McpProcessRegistry>,
    http: Client,
    key_store: AiProviderKeyStore,
}
