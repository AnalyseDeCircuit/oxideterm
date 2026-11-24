use std::{
    collections::HashSet,
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
const MCP_MAX_RETRIES: u32 = 3;
const MCP_RETRY_BASE_DELAY: Duration = Duration::from_secs(1);
const MCP_MAX_MULTI_ROUND_TRIPS: usize = 16;
const MCP_TASK_MAX_WAIT: Duration = Duration::from_secs(15 * 60);
const MCP_TASK_DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(1);
const MCP_TASK_MIN_POLL_INTERVAL: Duration = Duration::from_millis(100);
const MCP_TASK_MAX_POLL_INTERVAL: Duration = Duration::from_secs(30);

type PendingMap = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, McpError>>>>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum McpEffectiveTransport {
    Stdio,
    StreamableHttp,
    LegacySse,
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
    pub fn recovery(&self) -> crate::agent::Recovery {
        use crate::agent::Recovery;
        match self {
            Self::Timeout(_) => Recovery::Transient,
            Self::NotConnected(_) => Recovery::NeedsUser,
            Self::HttpStatus(status, _)
            | Self::Rpc {
                status: Some(status),
                ..
            } if status.as_u16() == 401 || status.as_u16() == 403 => Recovery::NeedsUser,
            Self::Rpc { code: -32602, .. } => Recovery::InvalidCall,
            _ => Recovery::OutcomeUnknown,
        }
    }

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
