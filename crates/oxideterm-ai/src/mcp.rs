use std::{
    collections::HashMap,
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
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

impl McpRegistry {
    pub fn new(key_store: AiProviderKeyStore) -> Self {
        Self {
            state: Arc::new(RwLock::new(McpRuntimeState::default())),
            processes: Arc::new(McpProcessRegistry::default()),
            http: Client::new(),
            key_store,
        }
    }

    pub async fn connect_all_values(&self, configs: &[Value]) {
        self.synchronize_values(configs).await;
    }

    pub async fn synchronize_values(&self, configs: &[Value]) {
        let parsed = configs
            .iter()
            .filter_map(|value| serde_json::from_value::<McpServerConfig>(value.clone()).ok())
            .collect::<Vec<_>>();
        self.synchronize_configs(parsed).await;
    }

    pub async fn synchronize_configs(&self, configs: Vec<McpServerConfig>) {
        let desired_ids = configs
            .iter()
            .map(|config| config.id.clone())
            .collect::<std::collections::HashSet<_>>();
        {
            let mut state = self.state.write();
            state.server_order = configs.iter().map(|config| config.id.clone()).collect();
        }
        let existing_ids = self
            .state
            .read()
            .servers
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for id in existing_ids {
            let should_remove = !desired_ids.contains(&id)
                || configs
                    .iter()
                    .find(|config| config.id == id)
                    .is_some_and(|config| !config.enabled);
            if should_remove {
                self.disconnect_and_remove(&id).await;
            }
        }

        let connect_tasks = configs
            .into_iter()
            .filter(|config| config.enabled)
            .map(|config| {
                let needs_reconnect = {
                    let state = self.state.read();
                    state.servers.get(&config.id).is_some_and(|current| {
                        current.config != config
                            && matches!(
                                current.status,
                                McpServerStatus::Connecting | McpServerStatus::Connected
                            )
                    })
                };
                let registry = self.clone();
                let task: BoxFuture<'static, ()> = async move {
                    if needs_reconnect {
                        registry.disconnect(&config.id).await;
                    }
                    registry.connect(config).await;
                }
                .boxed();
                task
            })
            .collect::<Vec<_>>();
        futures_util::future::join_all(connect_tasks).await;
    }

    pub async fn disconnect_all(&self) {
        let ids = self
            .state
            .read()
            .servers
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for id in ids {
            self.disconnect(&id).await;
        }
    }

    pub fn tool_definitions(&self) -> Vec<AiToolDefinition> {
        let state = self.state.read();
        let mut definitions = vec![
            AiToolDefinition {
                name: "list_mcp_resources".to_string(),
                description: "List all resources available from connected MCP servers. Returns URI, name, description, and MIME type for each resource.".to_string(),
                parameters: serde_json::json!({ "type": "object", "properties": {} }),
            },
            AiToolDefinition {
                name: "read_mcp_resource".to_string(),
                description: "Read the content of a specific MCP resource by its URI. Returns text or base64-encoded binary data.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "server_id": { "type": "string", "description": "The MCP server ID that owns this resource." },
                        "uri": { "type": "string", "description": "The resource URI to read (as returned by list_mcp_resources)." },
                    },
                    "required": ["server_id", "uri"],
                }),
            },
        ];
        definitions.extend(
            ordered_servers(&state)
                .into_iter()
                .filter(|server| server.status == McpServerStatus::Connected)
                .flat_map(|server| {
                    let namespace = server_namespace(server, &state.servers);
                    server.tools.iter().map(move |tool| AiToolDefinition {
                        name: format!("mcp::{namespace}::{}", tool.name),
                        description: format!(
                            "[MCP: {}] {}",
                            server.config.name,
                            tool.description.as_deref().unwrap_or(&tool.name)
                        ),
                        parameters: tool.input_schema.clone(),
                    })
                }),
        );
        definitions
    }

    pub fn resources(&self) -> Vec<(McpResource, String, String)> {
        let state = self.state.read();
        ordered_servers(&state)
            .into_iter()
            .filter(|server| server.status == McpServerStatus::Connected)
            .flat_map(|server| {
                server.resources.iter().cloned().map(|resource| {
                    (
                        resource,
                        server.config.id.clone(),
                        server.config.name.clone(),
                    )
                })
            })
            .collect()
    }

    pub fn snapshots(&self) -> Vec<McpServerStateSnapshot> {
        let state = self.state.read();
        ordered_servers(&state)
            .into_iter()
            .map(McpServerState::snapshot)
            .collect()
    }

    pub async fn connect_config(&self, config: McpServerConfig) {
        self.connect(config).await;
    }

    pub async fn disconnect_server(&self, server_id: &str) {
        self.disconnect(server_id).await;
    }

    pub fn has_auth_token(&self, server_id: &str) -> bool {
        self.key_store.has_provider_key(&format!("mcp:{server_id}"))
    }

    pub fn store_auth_token(
        &self,
        server_id: &str,
        token: Zeroizing<String>,
    ) -> anyhow::Result<()> {
        self.key_store
            .store_provider_key(&format!("mcp:{server_id}"), token)
    }

    pub fn delete_auth_token(&self, server_id: &str) -> anyhow::Result<()> {
        self.key_store
            .delete_provider_key(&format!("mcp:{server_id}"))
    }

    pub async fn call_prefixed_tool(
        &self,
        prefixed_name: &str,
        args: Value,
    ) -> Result<McpCallToolResult, McpError> {
        let (server_id, original_name) = self
            .state
            .read()
            .tool_index
            .get(prefixed_name)
            .cloned()
            .ok_or_else(|| {
            McpError::Message(format!("No MCP server found for tool: {prefixed_name}"))
        })?;
        let args = args.as_object().cloned().unwrap_or_default();
        let server = self.connected_server(&server_id)?;
        let generation = server.generation;
        let result = self
            .call_tool(&server, &original_name, Value::Object(args))
            .await;
        if result.is_err() {
            self.apply_runtime_error(
                &server_id,
                generation,
                result.as_ref().err().unwrap().to_string(),
            )
            .await;
        }
        result
    }

    pub async fn read_resource(
        &self,
        server_id: &str,
        uri: &str,
    ) -> Result<McpResourceContent, McpError> {
        let server = self.connected_server(server_id)?;
        let generation = server.generation;
        let result = self.read_resource_inner(&server, uri).await;
        if result.is_err() {
            self.apply_runtime_error(
                server_id,
                generation,
                result.as_ref().err().unwrap().to_string(),
            )
            .await;
        }
        result
    }

    pub async fn refresh_tools(&self, server_id: &str) -> Result<(), McpError> {
        let server = self.connected_server(server_id)?;
        let generation = server.generation;
        if server
            .capabilities
            .as_ref()
            .and_then(|cap| cap.tools.as_ref())
            .is_none()
        {
            return Ok(());
        }
        let tools = match self.list_tools(&server).await {
            Ok(tools) => tools,
            Err(error) => {
                self.apply_runtime_error(server_id, generation, error.to_string())
                    .await;
                return Err(error);
            }
        };
        let mut state = self.state.write();
        if let Some(current) = state.servers.get_mut(server_id)
            && current.status == McpServerStatus::Connected
            && current.generation == generation
        {
            current.tools = tools;
        }
        rebuild_tool_index(&mut state);
        Ok(())
    }

    async fn connect(&self, config: McpServerConfig) {
        let generation = {
            let mut state = self.state.write();
            if state.servers.get(&config.id).is_some_and(|server| {
                matches!(
                    server.status,
                    McpServerStatus::Connecting | McpServerStatus::Connected
                )
            }) {
                return;
            }
            let generation = state
                .generations
                .entry(config.id.clone())
                .and_modify(|value| *value = value.saturating_add(1))
                .or_insert(1);
            let generation = *generation;
            if !state.server_order.iter().any(|id| id == &config.id) {
                state.server_order.push(config.id.clone());
            }
            state.servers.insert(
                config.id.clone(),
                McpServerState {
                    status: McpServerStatus::Connecting,
                    ..McpServerState::disconnected(config.clone(), generation)
                },
            );
            generation
        };

        let result = self.connect_inner(config.clone(), generation).await;
        match result {
            Ok(server) => {
                let mut state = self.state.write();
                if current_generation(&state, &config.id) == generation {
                    state.retry_counters.remove(&config.id);
                    state.servers.insert(config.id.clone(), server);
                    rebuild_tool_index(&mut state);
                } else if let Some(runtime_id) = server.runtime_id {
                    let processes = self.processes.clone();
                    tokio::spawn(async move {
                        let _ = processes.close(&runtime_id).await;
                    });
                }
            }
            Err(error) => {
                let mut state = self.state.write();
                if current_generation(&state, &config.id) == generation {
                    state.servers.insert(
                        config.id.clone(),
                        McpServerState {
                            status: McpServerStatus::Error,
                            error: Some(error.to_string()),
                            ..McpServerState::disconnected(config.clone(), generation)
                        },
                    );
                    rebuild_tool_index(&mut state);
                    if should_retry_mcp_server(&config) {
                        drop(state);
                        self.schedule_retry(config.id.clone(), generation);
                    }
                }
            }
        }
    }

    async fn disconnect(&self, server_id: &str) {
        let current = {
            let mut state = self.state.write();
            let generation = state
                .generations
                .entry(server_id.to_string())
                .and_modify(|value| *value = value.saturating_add(1))
                .or_insert(1);
            let generation = *generation;
            state.retry_counters.remove(server_id);
            let current = state.servers.get(server_id).cloned();
            if let Some(existing) = state.servers.get_mut(server_id) {
                existing.status = McpServerStatus::Disconnected;
                existing.runtime_id = None;
                existing.endpoint_url = None;
                existing.session_id = None;
                existing.resolved_transport = None;
                existing.tools.clear();
                existing.resources.clear();
                existing.error = None;
                existing.generation = generation;
            }
            rebuild_tool_index(&mut state);
            current
        };
        if let Some(runtime_id) = current.and_then(|server| server.runtime_id) {
            let _ = self.processes.close(&runtime_id).await;
        }
    }

    async fn disconnect_and_remove(&self, server_id: &str) {
        self.disconnect(server_id).await;
        let mut state = self.state.write();
        state.servers.remove(server_id);
        state.server_order.retain(|id| id != server_id);
        state
            .tool_index
            .retain(|_, (owner_id, _)| owner_id.as_str() != server_id);
        state.retry_counters.remove(server_id);
    }

    async fn connect_inner(
        &self,
        config: McpServerConfig,
        generation: u64,
    ) -> Result<McpServerState, McpError> {
        match config.transport.effective() {
            McpEffectiveTransport::Stdio => self.connect_stdio(config, generation).await,
            McpEffectiveTransport::StreamableHttp | McpEffectiveTransport::LegacySse => {
                self.connect_http(config, generation).await
            }
        }
    }

    async fn connect_stdio(
        &self,
        config: McpServerConfig,
        generation: u64,
    ) -> Result<McpServerState, McpError> {
        let runtime_id = self
            .processes
            .spawn(
                config.command.as_deref().unwrap_or_default(),
                &config.args,
                &config.env,
            )
            .await?;
        let connected = async {
            let capabilities = self.initialize_stdio(&runtime_id).await?;
            let tools = if capabilities.tools.is_some() {
                self.stdio_list_tools(&runtime_id).await?
            } else {
                Vec::new()
            };
            let resources = if capabilities.resources.is_some() {
                self.stdio_list_resources(&runtime_id).await?
            } else {
                Vec::new()
            };
            Ok::<_, McpError>(McpServerState {
                config: config.clone(),
                status: McpServerStatus::Connected,
                error: None,
                capabilities: Some(capabilities),
                tools,
                resources,
                runtime_id: Some(runtime_id.clone()),
                endpoint_url: None,
                resolved_transport: Some(McpEffectiveTransport::Stdio),
                session_id: None,
                generation,
            })
        }
        .await;
        if connected.is_err() {
            let _ = self.processes.close(&runtime_id).await;
        }
        connected
    }

    async fn connect_http(
        &self,
        config: McpServerConfig,
        generation: u64,
    ) -> Result<McpServerState, McpError> {
        let token = self.mcp_auth_token(&config);
        let mut endpoint_url = config
            .url
            .clone()
            .ok_or_else(|| McpError::Message("MCP HTTP server requires url".to_string()))?;
        let mut resolved_transport = config.transport.effective();
        let mut init_request = json_rpc_request(
            "initialize",
            Some(serde_json::json!({
                "protocolVersion": resolved_transport.protocol_version(),
                "capabilities": {},
                "clientInfo": { "name": MCP_CLIENT_NAME, "version": MCP_CLIENT_VERSION },
            })),
        );
        let mut init_transport = if resolved_transport == McpEffectiveTransport::LegacySse {
            endpoint_url = self
                .discover_legacy_sse_endpoint(
                    &endpoint_url,
                    &config,
                    token.as_ref().map(|token| token.as_str()),
                )
                .await?;
            self.http_json_rpc_request(
                &endpoint_url,
                init_request.clone(),
                &config,
                token.as_ref().map(|token| token.as_str()),
                None,
                resolved_transport.protocol_version(),
                true,
            )
            .await?
        } else {
            match self
                .http_json_rpc_request(
                    &endpoint_url,
                    init_request.clone(),
                    &config,
                    token.as_ref().map(|token| token.as_str()),
                    None,
                    resolved_transport.protocol_version(),
                    true,
                )
                .await
            {
                Ok(response) => response,
                Err(McpError::HttpStatus(status, _))
                    if matches!(
                        status,
                        StatusCode::BAD_REQUEST
                            | StatusCode::NOT_FOUND
                            | StatusCode::METHOD_NOT_ALLOWED
                    ) =>
                {
                    resolved_transport = McpEffectiveTransport::LegacySse;
                    endpoint_url = self
                        .discover_legacy_sse_endpoint(
                            &endpoint_url,
                            &config,
                            token.as_ref().map(|token| token.as_str()),
                        )
                        .await?;
                    init_request = json_rpc_request(
                        "initialize",
                        Some(serde_json::json!({
                            "protocolVersion": resolved_transport.protocol_version(),
                            "capabilities": {},
                            "clientInfo": { "name": MCP_CLIENT_NAME, "version": MCP_CLIENT_VERSION },
                        })),
                    );
                    self.http_json_rpc_request(
                        &endpoint_url,
                        init_request,
                        &config,
                        token.as_ref().map(|token| token.as_str()),
                        None,
                        resolved_transport.protocol_version(),
                        true,
                    )
                    .await?
                }
                Err(error) => return Err(error),
            }
        };

        endpoint_url = init_transport.endpoint_url;
        let mut session_id = init_transport.session_id.take();
        let initialize_result = extract_result(init_transport.response.take())?;
        let capabilities = initialize_result
            .get("capabilities")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let capabilities = serde_json::from_value::<McpServerCapabilities>(capabilities)
            .map_err(|error| McpError::Message(error.to_string()))?;

        let notify = json_rpc_notification("notifications/initialized", None);
        let notify_transport = self
            .http_json_rpc_request(
                &endpoint_url,
                notify,
                &config,
                token.as_ref().map(|token| token.as_str()),
                session_id.as_deref(),
                resolved_transport.protocol_version(),
                false,
            )
            .await?;
        session_id = notify_transport.session_id.or(session_id);

        let mut server = McpServerState {
            config: config.clone(),
            status: McpServerStatus::Connected,
            error: None,
            capabilities: Some(capabilities),
            tools: Vec::new(),
            resources: Vec::new(),
            runtime_id: None,
            endpoint_url: Some(endpoint_url),
            resolved_transport: Some(resolved_transport),
            session_id,
            generation,
        };
        if server
            .capabilities
            .as_ref()
            .and_then(|cap| cap.tools.as_ref())
            .is_some()
        {
            let response = self.http_rpc(&server, "tools/list", None, true).await?;
            server.session_id = response.session_id.or(server.session_id);
            server.tools = parse_tools(extract_result(response.response)?)?;
        }
        if server
            .capabilities
            .as_ref()
            .and_then(|cap| cap.resources.as_ref())
            .is_some()
        {
            let response = self.http_rpc(&server, "resources/list", None, true).await?;
            server.session_id = response.session_id.or(server.session_id);
            server.resources = parse_resources(extract_result(response.response)?)?;
        }
        Ok(server)
    }

    fn connected_server(&self, server_id: &str) -> Result<McpServerState, McpError> {
        let state = self.state.read();
        let server = state
            .servers
            .get(server_id)
            .cloned()
            .ok_or_else(|| McpError::NotConnected(server_id.to_string()))?;
        if server.status != McpServerStatus::Connected {
            return Err(McpError::NotConnected(server_id.to_string()));
        }
        Ok(server)
    }

    async fn apply_runtime_error(&self, server_id: &str, generation: u64, message: String) {
        let (runtime_id, config, generation) = {
            let mut state = self.state.write();
            let Some(server) = state.servers.get_mut(server_id) else {
                return;
            };
            if server.generation != generation {
                return;
            }
            let config = server.config.clone();
            server.status = McpServerStatus::Error;
            server.error = Some(message);
            server.tools.clear();
            server.resources.clear();
            (server.runtime_id.take(), config, generation)
        };
        if let Some(runtime_id) = runtime_id {
            let _ = self.processes.close(&runtime_id).await;
        }
        rebuild_tool_index(&mut self.state.write());
        if should_retry_mcp_server(&config) {
            self.schedule_retry(server_id.to_string(), generation);
        }
    }

    fn schedule_retry(&self, server_id: String, generation: u64) {
        let (config, attempt) = {
            let mut state = self.state.write();
            if current_generation(&state, &server_id) != generation {
                return;
            }
            let Some(config) = state
                .servers
                .get(&server_id)
                .map(|server| server.config.clone())
            else {
                return;
            };
            if !should_retry_mcp_server(&config) {
                state.retry_counters.remove(&server_id);
                return;
            }
            let attempt = state
                .retry_counters
                .entry(server_id.clone())
                .and_modify(|value| *value = value.saturating_add(1))
                .or_insert(1);
            let attempt = *attempt;
            if attempt > MCP_MAX_RETRIES {
                tracing::warn!(
                    "[MCP:{}] giving up retry after {} attempts",
                    server_id,
                    MCP_MAX_RETRIES
                );
                state.retry_counters.remove(&server_id);
                return;
            }
            (config, attempt)
        };
        let delay = MCP_RETRY_BASE_DELAY * 2_u32.saturating_pow(attempt.saturating_sub(1));
        let registry = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            let should_connect = {
                let state = registry.state.read();
                current_generation(&state, &server_id) == generation
                    && state.servers.get(&server_id).is_some_and(|server| {
                        should_retry_mcp_server(&server.config)
                            && !matches!(
                                server.status,
                                McpServerStatus::Connected | McpServerStatus::Connecting
                            )
                    })
            };
            if should_connect {
                registry.connect(config).await;
            }
        });
    }

    async fn initialize_stdio(&self, runtime_id: &str) -> Result<McpServerCapabilities, McpError> {
        let init = self
            .processes
            .send_request(
                runtime_id,
                "initialize",
                serde_json::json!({
                    "protocolVersion": LEGACY_SSE_PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": { "name": MCP_CLIENT_NAME, "version": MCP_CLIENT_VERSION },
                }),
            )
            .await?;
        let capabilities = init
            .get("capabilities")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        self.processes
            .send_request(
                runtime_id,
                "notifications/initialized",
                serde_json::json!({}),
            )
            .await?;
        serde_json::from_value(capabilities).map_err(|error| McpError::Message(error.to_string()))
    }

    async fn stdio_list_tools(&self, runtime_id: &str) -> Result<Vec<McpToolSchema>, McpError> {
        let result = self
            .processes
            .send_request(runtime_id, "tools/list", serde_json::json!({}))
            .await?;
        parse_tools(result)
    }

    async fn stdio_list_resources(&self, runtime_id: &str) -> Result<Vec<McpResource>, McpError> {
        let result = self
            .processes
            .send_request(runtime_id, "resources/list", serde_json::json!({}))
            .await?;
        parse_resources(result)
    }

    async fn list_tools(&self, server: &McpServerState) -> Result<Vec<McpToolSchema>, McpError> {
        if let Some(runtime_id) = &server.runtime_id {
            return self.stdio_list_tools(runtime_id).await;
        }
        let response = self.http_rpc(server, "tools/list", None, true).await?;
        parse_tools(extract_result(response.response)?)
    }

    async fn call_tool(
        &self,
        server: &McpServerState,
        tool_name: &str,
        args: Value,
    ) -> Result<McpCallToolResult, McpError> {
        let params = serde_json::json!({ "name": tool_name, "arguments": args });
        let result = if let Some(runtime_id) = &server.runtime_id {
            self.processes
                .send_request(runtime_id, "tools/call", params)
                .await?
        } else {
            extract_result(
                self.http_rpc(server, "tools/call", Some(params), true)
                    .await?
                    .response,
            )?
        };
        serde_json::from_value(result).map_err(|error| McpError::Message(error.to_string()))
    }

    async fn read_resource_inner(
        &self,
        server: &McpServerState,
        uri: &str,
    ) -> Result<McpResourceContent, McpError> {
        let params = serde_json::json!({ "uri": uri });
        let result = if let Some(runtime_id) = &server.runtime_id {
            self.processes
                .send_request(runtime_id, "resources/read", params)
                .await?
        } else {
            extract_result(
                self.http_rpc(server, "resources/read", Some(params), true)
                    .await?
                    .response,
            )?
        };
        let contents = result
            .get("contents")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let Some(first) = contents.into_iter().next() else {
            return Err(McpError::Message(format!(
                "Empty resource response for {uri}"
            )));
        };
        serde_json::from_value(first).map_err(|error| McpError::Message(error.to_string()))
    }

    async fn http_rpc(
        &self,
        server: &McpServerState,
        method: &str,
        params: Option<Value>,
        expect_json: bool,
    ) -> Result<HttpRequestResult, McpError> {
        let transport = server
            .resolved_transport
            .unwrap_or_else(|| server.config.transport.effective());
        let endpoint = server
            .endpoint_url
            .as_deref()
            .or(server.config.url.as_deref())
            .ok_or_else(|| McpError::Message("MCP HTTP server requires url".to_string()))?;
        let token = self.mcp_auth_token(&server.config);
        self.http_json_rpc_request(
            endpoint,
            if method.starts_with("notifications/") {
                json_rpc_notification(method, params)
            } else {
                json_rpc_request(method, params)
            },
            &server.config,
            token.as_ref().map(|token| token.as_str()),
            server.session_id.as_deref(),
            transport.protocol_version(),
            expect_json,
        )
        .await
    }

    fn mcp_auth_token(&self, config: &McpServerConfig) -> Option<Zeroizing<String>> {
        // Tauri stores MCP auth tokens in the same OS keychain namespace under
        // `mcp:{id}` and only falls back to legacy config.authToken for
        // migration. Keep both values out of Debug/log paths and return an
        // owned Zeroizing clone with request-scoped lifetime.
        self.key_store
            .get_provider_key(&format!("mcp:{}", config.id))
            .ok()
            .flatten()
            .or_else(|| {
                config
                    .auth_token
                    .as_ref()
                    .map(|token| Zeroizing::new(token.clone()))
            })
    }

    async fn discover_legacy_sse_endpoint(
        &self,
        base_url: &str,
        config: &McpServerConfig,
        auth_token: Option<&str>,
    ) -> Result<String, McpError> {
        let url = validate_mcp_http_url(base_url)?;
        let headers = build_http_headers(
            config,
            auth_token,
            None,
            LEGACY_SSE_PROTOCOL_VERSION,
            false,
            "text/event-stream",
        )?;
        let response = tokio::time::timeout(
            MCP_REQUEST_TIMEOUT,
            self.http.get(&url).headers(headers).send(),
        )
        .await
        .map_err(|_| McpError::Timeout(url.clone()))?
        .map_err(|error| McpError::Message(error.to_string()))?;
        if !response.status().is_success() {
            return Err(McpError::HttpStatus(
                response.status(),
                response
                    .status()
                    .canonical_reason()
                    .unwrap_or("")
                    .to_string(),
            ));
        }
        let endpoint = tokio::time::timeout(MCP_REQUEST_TIMEOUT, read_sse_until_endpoint(response))
            .await
            .map_err(|_| McpError::Timeout(url.clone()))??;
        let base =
            reqwest::Url::parse(&url).map_err(|error| McpError::Message(error.to_string()))?;
        base.join(&endpoint)
            .map(|url| url.to_string())
            .map_err(|error| McpError::Message(error.to_string()))
    }

    #[allow(clippy::too_many_arguments)]
    async fn http_json_rpc_request(
        &self,
        endpoint_url: &str,
        request: Value,
        config: &McpServerConfig,
        auth_token: Option<&str>,
        session_id: Option<&str>,
        protocol_version: &str,
        expect_json: bool,
    ) -> Result<HttpRequestResult, McpError> {
        let url = validate_mcp_http_url(endpoint_url)?;
        let request_id = request.get("id").and_then(Value::as_u64);
        let headers = build_http_headers(
            config,
            auth_token,
            session_id,
            protocol_version,
            true,
            "application/json, text/event-stream",
        )?;
        tokio::time::timeout(MCP_REQUEST_TIMEOUT, async {
            let response = self
                .http
                .post(&url)
                .headers(headers)
                .json(&request)
                .send()
                .await
                .map_err(|error| McpError::Message(error.to_string()))?;
            if !response.status().is_success() {
                return Err(McpError::HttpStatus(
                    response.status(),
                    response
                        .status()
                        .canonical_reason()
                        .unwrap_or("")
                        .to_string(),
                ));
            }
            let session_id = response
                .headers()
                .get("MCP-Session-Id")
                .or_else(|| response.headers().get("Mcp-Session-Id"))
                .and_then(|value| value.to_str().ok())
                .map(str::to_string)
                .or_else(|| session_id.map(str::to_string));
            let response = parse_http_response(response, request_id, expect_json).await?;
            Ok(HttpRequestResult {
                endpoint_url: url,
                session_id,
                response,
            })
        })
        .await
        .map_err(|_| McpError::Timeout(endpoint_url.to_string()))?
    }
}

impl McpProcessRegistry {
    async fn stop_all(&self) {
        let ids = self
            .processes
            .lock()
            .await
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for id in ids {
            let _ = self.close(&id).await;
        }
    }

    async fn spawn(
        &self,
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
    ) -> Result<String, McpError> {
        validate_mcp_command(command)?;
        validate_mcp_env(env)?;
        let server_id = format!("mcp-{}", uuid::Uuid::new_v4());

        let mut cmd = Command::new(command);
        cmd.args(args)
            .env_clear()
            .envs(env)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        if let Ok(path) = std::env::var("PATH") {
            cmd.env("PATH", path);
        }
        if let Ok(home) = std::env::var("HOME") {
            cmd.env("HOME", home);
        }

        let mut child = cmd.spawn().map_err(|error| {
            McpError::Message(format!("Failed to spawn MCP server '{command}': {error}"))
        })?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| McpError::Message("Failed to capture stdin".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| McpError::Message("Failed to capture stdout".to_string()))?;
        let stderr_task = if let Some(stderr) = child.stderr.take() {
            let sid = server_id.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr);
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line).await {
                        Ok(0) | Err(_) => break,
                        Ok(_) => tracing::debug!("[MCP:{sid}] stderr: {}", line.trim_end()),
                    }
                }
            })
        } else {
            tokio::spawn(async {})
        };

        let pending = Arc::new(Mutex::new(HashMap::new()));
        let reader_task = {
            let pending = pending.clone();
            let sid = server_id.clone();
            tokio::spawn(stdout_reader_loop(BufReader::new(stdout), pending, sid))
        };
        self.processes.lock().await.insert(
            server_id.clone(),
            Arc::new(McpProcess {
                child: Mutex::new(child),
                stdin: Mutex::new(stdin),
                next_id: AtomicU64::new(1),
                pending,
                reader_task,
                stderr_task,
            }),
        );
        Ok(server_id)
    }

    async fn send_request(
        &self,
        server_id: &str,
        method: &str,
        params: Value,
    ) -> Result<Value, McpError> {
        let process = self
            .processes
            .lock()
            .await
            .get(server_id)
            .cloned()
            .ok_or_else(|| McpError::Message(format!("MCP server {server_id} not found")))?;
        let is_notification = method.starts_with("notifications/");
        let request_id = process.next_id.fetch_add(1, Ordering::Relaxed);
        let request = if is_notification {
            serde_json::json!({ "jsonrpc": "2.0", "method": method, "params": params })
        } else {
            serde_json::json!({ "jsonrpc": "2.0", "id": request_id, "method": method, "params": params })
        };
        let body = serde_json::to_string(&request)
            .map_err(|error| McpError::Message(error.to_string()))?;
        let rx = if is_notification {
            None
        } else {
            let (tx, rx) = oneshot::channel();
            process.pending.lock().await.insert(request_id, tx);
            Some(rx)
        };
        {
            let mut stdin = process.stdin.lock().await;
            if let Err(error) = write_framed_message(&mut *stdin, &body).await {
                if !is_notification {
                    process.pending.lock().await.remove(&request_id);
                }
                return Err(error);
            }
        }
        let Some(rx) = rx else {
            return Ok(Value::Null);
        };
        match tokio::time::timeout(MCP_REQUEST_TIMEOUT, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(McpError::Message(format!(
                "MCP server {server_id} connection lost"
            ))),
            Err(_) => {
                process.pending.lock().await.remove(&request_id);
                Err(McpError::Timeout(server_id.to_string()))
            }
        }
    }

    async fn close(&self, server_id: &str) -> Result<(), McpError> {
        let process = self.processes.lock().await.remove(server_id);
        let Some(process) = process else {
            return Ok(());
        };
        let id = process.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        process.pending.lock().await.insert(id, tx);
        let shutdown = format!(r#"{{"jsonrpc":"2.0","id":{id},"method":"shutdown"}}"#);
        let write_ok = {
            let mut stdin = process.stdin.lock().await;
            write_framed_message(&mut *stdin, &shutdown).await.is_ok()
        };
        if write_ok {
            let _ = tokio::time::timeout(MCP_SHUTDOWN_TIMEOUT, rx).await;
        } else {
            process.pending.lock().await.remove(&id);
        }
        {
            let mut stdin = process.stdin.lock().await;
            let _ = write_framed_message(&mut *stdin, r#"{"jsonrpc":"2.0","method":"exit"}"#).await;
        }
        let _ = process.child.lock().await.kill().await;
        process.reader_task.abort();
        process.stderr_task.abort();
        for (_, tx) in process.pending.lock().await.drain() {
            let _ = tx.send(Err(McpError::Message("MCP server closed".to_string())));
        }
        Ok(())
    }
}

impl Drop for McpRegistry {
    fn drop(&mut self) {
        let processes = self.processes.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                processes.stop_all().await;
            });
        }
    }
}

async fn stdout_reader_loop<R>(mut reader: R, pending: PendingMap, server_id: String)
where
    R: AsyncBufRead + Unpin,
{
    let mut header_line = String::new();
    loop {
        header_line.clear();
        let bytes_read = match reader.read_line(&mut header_line).await {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        let trimmed = header_line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let body = if trimmed.starts_with('{') || trimmed.starts_with('[') {
            let _ = bytes_read;
            trimmed.to_string()
        } else {
            let mut headers = vec![trimmed.to_string()];
            let mut next = String::new();
            loop {
                next.clear();
                match reader.read_line(&mut next).await {
                    Ok(0) => break,
                    Ok(_) if next.trim().is_empty() => break,
                    Ok(_) => headers.push(next.trim().to_string()),
                    Err(_) => break,
                }
            }
            let Some(length) = headers.iter().find_map(|header| {
                let (name, value) = header.split_once(':')?;
                name.trim()
                    .eq_ignore_ascii_case("content-length")
                    .then_some(value.trim())
            }) else {
                break;
            };
            let Ok(length) = length.parse::<usize>() else {
                break;
            };
            if length == 0 || length > MAX_MCP_MESSAGE_BYTES {
                break;
            }
            let mut buf = vec![0u8; length];
            if tokio::io::AsyncReadExt::read_exact(&mut reader, &mut buf)
                .await
                .is_err()
            {
                break;
            }
            String::from_utf8_lossy(&buf).into_owned()
        };
        let Ok(value) = serde_json::from_str::<Value>(body.trim()) else {
            continue;
        };
        let Some(id) = value.get("id").and_then(Value::as_u64) else {
            tracing::debug!(
                "[MCP:{server_id}] notification: {}",
                value
                    .get("method")
                    .and_then(|value| value.as_str())
                    .unwrap_or("unknown")
            );
            continue;
        };
        let tx = pending.lock().await.remove(&id);
        if let Some(tx) = tx {
            if let Some(error) = value.get("error") {
                let message = error
                    .get("message")
                    .and_then(|value| value.as_str())
                    .unwrap_or("Unknown MCP error");
                let _ = tx.send(Err(McpError::Message(format!("MCP error: {message}"))));
            } else if let Some(result) = value.get("result") {
                let _ = tx.send(Ok(result.clone()));
            } else {
                let _ = tx.send(Err(McpError::Message(
                    "MCP response missing result".to_string(),
                )));
            }
        }
    }
    for (_, tx) in pending.lock().await.drain() {
        let _ = tx.send(Err(McpError::Message(
            "MCP server closed stdout".to_string(),
        )));
    }
}

async fn write_framed_message<W>(writer: &mut W, body: &str) -> Result<(), McpError>
where
    W: AsyncWrite + Unpin,
{
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    writer.write_all(header.as_bytes()).await.map_err(|error| {
        McpError::Message(format!("Failed to write header to MCP server: {error}"))
    })?;
    writer
        .write_all(body.as_bytes())
        .await
        .map_err(|error| McpError::Message(format!("Failed to write to MCP server: {error}")))?;
    writer
        .flush()
        .await
        .map_err(|error| McpError::Message(format!("Failed to flush: {error}")))
}

fn validate_mcp_command(command: &str) -> Result<(), McpError> {
    const ALLOWED: &[&str] = &["npx", "uvx", "docker"];
    let command = command.trim();
    if command.is_empty() {
        return Err(McpError::Message(
            "MCP command must not be empty".to_string(),
        ));
    }
    let basename = std::path::Path::new(command)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if basename != command {
        return Err(McpError::Message(format!(
            "MCP command must be a plain command name (no paths). Got: '{command}'"
        )));
    }
    if !ALLOWED.contains(&basename) {
        return Err(McpError::Message(format!(
            "MCP command '{basename}' is not in the allowlist. Allowed: {ALLOWED:?}"
        )));
    }
    Ok(())
}

fn validate_mcp_env(env: &HashMap<String, String>) -> Result<(), McpError> {
    const BLOCKED: &[&str] = &[
        "LD_PRELOAD",
        "DYLD_INSERT_LIBRARIES",
        "LD_LIBRARY_PATH",
        "NODE_OPTIONS",
        "PYTHONPATH",
        "PYTHONSTARTUP",
    ];
    for key in env.keys() {
        if BLOCKED.contains(&key.trim().to_ascii_uppercase().as_str()) {
            return Err(McpError::Message(format!(
                "MCP env variable '{key}' is not allowed"
            )));
        }
    }
    Ok(())
}

fn build_http_headers(
    config: &McpServerConfig,
    auth_token: Option<&str>,
    session_id: Option<&str>,
    protocol_version: &str,
    content_type: bool,
    accept: &str,
) -> Result<reqwest::header::HeaderMap, McpError> {
    const RESERVED: &[&str] = &[
        "accept",
        "content-type",
        "mcp-session-id",
        "mcp-protocol-version",
    ];
    let mut headers = reqwest::header::HeaderMap::new();
    let auth_header_name = config
        .auth_header_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Authorization");
    let normalized_auth = auth_header_name.to_ascii_lowercase();
    for (raw_name, raw_value) in &config.headers {
        let name = raw_name.trim();
        if name.is_empty() {
            continue;
        }
        let normalized = name.to_ascii_lowercase();
        if RESERVED.contains(&normalized.as_str()) || normalized == normalized_auth {
            continue;
        }
        let header = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| McpError::Message(format!("Invalid MCP HTTP header name: {raw_name}")))?;
        let value = raw_value.parse().map_err(|_| {
            McpError::Message(format!("Invalid MCP HTTP header value for {raw_name}"))
        })?;
        headers.insert(header, value);
    }
    if let Some(token) = auth_token
        && config.auth_header_mode.unwrap_or(McpAuthHeaderMode::Bearer) != McpAuthHeaderMode::None
    {
        let header = HeaderName::from_bytes(auth_header_name.as_bytes()).map_err(|_| {
            McpError::Message(format!("Invalid MCP auth header name: {auth_header_name}"))
        })?;
        let value = match config.auth_header_mode.unwrap_or(McpAuthHeaderMode::Bearer) {
            McpAuthHeaderMode::Bearer => format!("Bearer {token}"),
            McpAuthHeaderMode::Raw => token.to_string(),
            McpAuthHeaderMode::None => String::new(),
        };
        headers.insert(
            header,
            value.parse().map_err(|_| {
                McpError::Message(format!(
                    "Invalid MCP auth header value for {auth_header_name}"
                ))
            })?,
        );
    }
    headers.insert(
        reqwest::header::ACCEPT,
        accept.parse().expect("valid Accept"),
    );
    if content_type {
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            "application/json".parse().expect("valid content type"),
        );
    }
    headers.insert(
        HeaderName::from_static("mcp-protocol-version"),
        protocol_version.parse().expect("valid protocol version"),
    );
    if let Some(session_id) = session_id {
        headers.insert(
            HeaderName::from_static("mcp-session-id"),
            session_id
                .parse()
                .map_err(|_| McpError::Message("Invalid MCP session id".to_string()))?,
        );
    }
    Ok(headers)
}

#[derive(Default)]
struct HttpRequestResult {
    endpoint_url: String,
    session_id: Option<String>,
    response: Option<Value>,
}

fn json_rpc_request(method: &str, params: Option<Value>) -> Value {
    static NEXT_HTTP_ID: AtomicU64 = AtomicU64::new(1);
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": NEXT_HTTP_ID.fetch_add(1, Ordering::Relaxed),
        "method": method,
        "params": params.unwrap_or_else(|| serde_json::json!({})),
    })
}

fn json_rpc_notification(method: &str, params: Option<Value>) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params.unwrap_or_else(|| serde_json::json!({})),
    })
}

async fn parse_http_response(
    response: reqwest::Response,
    request_id: Option<u64>,
    expect_json: bool,
) -> Result<Option<Value>, McpError> {
    if !expect_json
        || matches!(
            response.status(),
            StatusCode::ACCEPTED | StatusCode::NO_CONTENT
        )
    {
        return Ok(None);
    }
    let is_sse = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.contains("text/event-stream"));
    if is_sse {
        return read_sse_matching_response(response, request_id)
            .await
            .map(Some);
    }
    let text = response
        .text()
        .await
        .map_err(|error| McpError::Message(error.to_string()))?;
    if text.trim().is_empty() {
        return Ok(None);
    }
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|error| McpError::Message(error.to_string()))
}

fn extract_result(response: Option<Value>) -> Result<Value, McpError> {
    let Some(response) = response else {
        return Err(McpError::Message("MCP response missing result".to_string()));
    };
    if let Some(error) = response.get("error") {
        let code = error
            .get("code")
            .and_then(|value| value.as_i64())
            .unwrap_or_default();
        let message = error
            .get("message")
            .and_then(|value| value.as_str())
            .unwrap_or("Unknown MCP error");
        return Err(McpError::Message(format!("MCP error {code}: {message}")));
    }
    response
        .get("result")
        .cloned()
        .ok_or_else(|| McpError::Message("MCP response missing result".to_string()))
}

fn parse_tools(value: Value) -> Result<Vec<McpToolSchema>, McpError> {
    serde_json::from_value(
        value
            .get("tools")
            .cloned()
            .unwrap_or_else(|| serde_json::json!([])),
    )
    .map_err(|error| McpError::Message(error.to_string()))
}

fn parse_resources(value: Value) -> Result<Vec<McpResource>, McpError> {
    serde_json::from_value(
        value
            .get("resources")
            .cloned()
            .unwrap_or_else(|| serde_json::json!([])),
    )
    .map_err(|error| McpError::Message(error.to_string()))
}

async fn read_sse_matching_response(
    response: reqwest::Response,
    request_id: Option<u64>,
) -> Result<Value, McpError> {
    let mut stream = response.bytes_stream();
    let mut parser = SseEventParser::default();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| McpError::Message(error.to_string()))?;
        parser.push_str(&String::from_utf8_lossy(&chunk));
        for event in parser.drain_events() {
            if let Ok(value) = serde_json::from_str::<Value>(&event.data)
                && request_id.is_none_or(|id| value.get("id").and_then(Value::as_u64) == Some(id))
            {
                return Ok(value);
            }
        }
    }
    for event in parser.finish() {
        if let Ok(value) = serde_json::from_str::<Value>(&event.data)
            && request_id.is_none_or(|id| value.get("id").and_then(Value::as_u64) == Some(id))
        {
            return Ok(value);
        }
    }
    Err(McpError::Message(
        "MCP SSE stream ended without a matching JSON-RPC response".to_string(),
    ))
}

async fn read_sse_until_endpoint(response: reqwest::Response) -> Result<String, McpError> {
    let mut stream = response.bytes_stream();
    let mut parser = SseEventParser::default();
    while let Some(chunk) = tokio::time::timeout(MCP_REQUEST_TIMEOUT, stream.next())
        .await
        .map_err(|_| McpError::Message("Legacy MCP SSE endpoint discovery timed out".to_string()))?
    {
        let chunk = chunk.map_err(|error| McpError::Message(error.to_string()))?;
        parser.push_str(&String::from_utf8_lossy(&chunk));
        for event in parser.drain_events() {
            if event.name == "endpoint" && !event.data.trim().is_empty() {
                return Ok(event.data.trim().to_string());
            }
        }
    }
    for event in parser.finish() {
        if event.name == "endpoint" && !event.data.trim().is_empty() {
            return Ok(event.data.trim().to_string());
        }
    }
    Err(McpError::Message(
        "Legacy MCP SSE endpoint discovery failed: missing endpoint event".to_string(),
    ))
}

#[derive(Debug)]
struct SseEvent {
    name: String,
    data: String,
}

#[derive(Debug)]
struct SseEventParser {
    buffer: String,
    event_name: String,
    data_lines: Vec<String>,
}

impl Default for SseEventParser {
    fn default() -> Self {
        Self {
            buffer: String::new(),
            event_name: "message".to_string(),
            data_lines: Vec::new(),
        }
    }
}

impl SseEventParser {
    fn push_str(&mut self, chunk: &str) {
        self.buffer.push_str(chunk);
    }

    fn drain_events(&mut self) -> Vec<SseEvent> {
        let mut events = Vec::new();
        while let Some(index) = self.buffer.find('\n') {
            let mut line = self.buffer[..index].to_string();
            self.buffer = self.buffer[index + 1..].to_string();
            if line.ends_with('\r') {
                line.pop();
            }

            if line.is_empty() {
                if let Some(event) = self.flush_event() {
                    events.push(event);
                }
            } else if line.starts_with(':') {
                continue;
            } else if let Some(value) = line.strip_prefix("event:") {
                self.event_name = value.trim().to_string();
                if self.event_name.is_empty() {
                    self.event_name = "message".to_string();
                }
            } else if let Some(value) = line.strip_prefix("data:") {
                self.data_lines.push(value.trim_start().to_string());
            }
        }
        events
    }

    fn finish(mut self) -> Vec<SseEvent> {
        if let Some(value) = self.buffer.strip_prefix("data:") {
            self.data_lines.push(value.trim_start().to_string());
        } else if !self.buffer.trim().is_empty() {
            self.buffer.clear();
        }
        self.flush_event().into_iter().collect()
    }

    fn flush_event(&mut self) -> Option<SseEvent> {
        if self.data_lines.is_empty() {
            self.event_name = "message".to_string();
            return None;
        }
        Some(SseEvent {
            name: std::mem::replace(&mut self.event_name, "message".to_string()),
            data: std::mem::take(&mut self.data_lines).join("\n"),
        })
    }
}

fn server_namespace(server: &McpServerState, servers: &HashMap<String, McpServerState>) -> String {
    let count = servers
        .values()
        .filter(|other| {
            other.status == McpServerStatus::Connected && other.config.name == server.config.name
        })
        .count();
    if count <= 1 {
        server.config.name.clone()
    } else {
        format!("{}#{}", server.config.name, server.config.id)
    }
}

fn rebuild_tool_index(state: &mut McpRuntimeState) {
    state.tool_index.clear();
    for server in state
        .servers
        .values()
        .filter(|server| server.status == McpServerStatus::Connected)
    {
        let namespace = server_namespace(server, &state.servers);
        for tool in &server.tools {
            state.tool_index.insert(
                format!("mcp::{namespace}::{}", tool.name),
                (server.config.id.clone(), tool.name.clone()),
            );
        }
    }
}

fn ordered_servers(state: &McpRuntimeState) -> Vec<&McpServerState> {
    let mut servers = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for id in &state.server_order {
        if let Some(server) = state.servers.get(id) {
            seen.insert(id.as_str());
            servers.push(server);
        }
    }
    let mut remaining = state
        .servers
        .iter()
        .filter(|(id, _)| !seen.contains(id.as_str()))
        .collect::<Vec<_>>();
    remaining.sort_by(|(left, _), (right, _)| left.cmp(right));
    servers.extend(remaining.into_iter().map(|(_, server)| server));
    servers
}

fn current_generation(state: &McpRuntimeState, id: &str) -> u64 {
    state.generations.get(id).copied().unwrap_or_default()
}

fn validate_mcp_http_url(url: &str) -> Result<String, McpError> {
    let parsed = reqwest::Url::parse(url).map_err(|error| McpError::Message(error.to_string()))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(McpError::Message(
            "MCP HTTP only supports http/https URLs".to_string(),
        ));
    }
    Ok(parsed.to_string())
}

fn should_retry_mcp_server(config: &McpServerConfig) -> bool {
    config.enabled
        && config.retry_on_disconnect
        && !matches!(config.transport.effective(), McpEffectiveTransport::Stdio)
}

fn redacted_mcp_config(config: &McpServerConfig) -> McpServerConfig {
    let mut redacted = config.clone();
    redacted.auth_token = None;
    for value in redacted.env.values_mut() {
        *value = "[redacted]".to_string();
    }
    for value in redacted.headers.values_mut() {
        *value = "[redacted]".to_string();
    }
    redacted.args = redact_sensitive_args(&redacted.args);
    redacted
}

fn redact_sensitive_args(args: &[String]) -> Vec<String> {
    let mut redact_next = false;
    args.iter()
        .map(|arg| {
            if redact_next {
                redact_next = false;
                return "[redacted]".to_string();
            }
            let lower = arg.to_ascii_lowercase();
            if matches!(
                lower.as_str(),
                "--api-key" | "--token" | "--secret" | "--password"
            ) || lower.ends_with("_token")
                || lower.ends_with("_key")
            {
                redact_next = true;
            }
            if lower.contains("token=") || lower.contains("api_key=") || lower.contains("password=")
            {
                "[redacted]".to_string()
            } else {
                arg.clone()
            }
        })
        .collect()
}

pub fn is_mcp_tool_name(name: &str) -> bool {
    name.starts_with("mcp::")
}

pub fn mcp_tool_output(result: &McpCallToolResult) -> (bool, String, bool) {
    let text = result
        .content
        .iter()
        .filter(|content| content.content_type == "text")
        .filter_map(|content| content.text.as_deref())
        .collect::<Vec<_>>()
        .join("\n");
    let truncated = !result.is_error && text.chars().count() > MCP_TOOL_OUTPUT_MAX_CHARS;
    let output = truncate_chars(&text, MCP_TOOL_OUTPUT_MAX_CHARS);
    (!result.is_error, output, truncated)
}

pub fn mcp_resource_output(content: &McpResourceContent) -> (String, bool) {
    let text = content.text.clone().unwrap_or_else(|| {
        content
            .blob
            .as_ref()
            .map(|blob| {
                format!(
                    "[base64 binary, {} chars, mime={}]",
                    blob.len(),
                    content.mime_type.as_deref().unwrap_or("unknown")
                )
            })
            .unwrap_or_else(|| "(empty)".to_string())
    });
    let truncated = text.chars().count() > MCP_TOOL_OUTPUT_MAX_CHARS;
    (truncate_chars(&text, MCP_TOOL_OUTPUT_MAX_CHARS), truncated)
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt, duplex},
        net::TcpListener,
    };

    #[tokio::test]
    async fn stdout_reader_dispatches_content_length_framed_response() {
        let (client, mut server) = duplex(1024);
        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let (tx, rx) = oneshot::channel();
        pending.lock().await.insert(7, tx);
        let task = tokio::spawn(stdout_reader_loop(
            BufReader::new(client),
            pending,
            "test".to_string(),
        ));
        let body = r#"{"jsonrpc":"2.0","id":7,"result":{"ok":true}}"#;
        let message = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
        server.write_all(message.as_bytes()).await.unwrap();
        let result = rx.await.unwrap().unwrap();
        assert_eq!(result["ok"].as_bool(), Some(true));
        drop(server);
        let _ = task.await;
    }

    #[tokio::test]
    async fn stdout_reader_dispatches_line_delimited_response() {
        let (client, mut server) = duplex(1024);
        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let (tx, rx) = oneshot::channel();
        pending.lock().await.insert(3, tx);
        let task = tokio::spawn(stdout_reader_loop(
            BufReader::new(client),
            pending,
            "line-json".to_string(),
        ));

        server
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":3,\"result\":{\"ok\":true}}\n")
            .await
            .unwrap();
        let result = rx.await.unwrap().unwrap();
        assert_eq!(result["ok"].as_bool(), Some(true));
        drop(server);
        let _ = task.await;
    }

    #[tokio::test]
    async fn stdout_reader_rejects_pending_when_stdout_closes() {
        let (client, server) = duplex(256);
        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let (tx, rx) = oneshot::channel();
        pending.lock().await.insert(1, tx);
        let task = tokio::spawn(stdout_reader_loop(
            BufReader::new(client),
            pending,
            "close".to_string(),
        ));

        drop(server);

        let error = rx.await.unwrap().unwrap_err();
        assert_eq!(error.to_string(), "MCP server closed stdout");
        let _ = task.await;
    }

    #[tokio::test]
    async fn stdout_reader_treats_invalid_content_length_as_fatal() {
        let (client, mut server) = duplex(1024);
        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let (tx, rx) = oneshot::channel();
        pending.lock().await.insert(9, tx);
        let task = tokio::spawn(stdout_reader_loop(
            BufReader::new(client),
            pending,
            "invalid-length".to_string(),
        ));

        server
            .write_all(b"Content-Length: 999999999\r\n\r\n{}")
            .await
            .unwrap();
        drop(server);

        let error = rx.await.unwrap().unwrap_err();
        assert_eq!(error.to_string(), "MCP server closed stdout");
        let _ = task.await;
    }

    #[tokio::test]
    async fn stdout_reader_rejects_response_without_result_or_error() {
        let (client, mut server) = duplex(1024);
        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let (tx, rx) = oneshot::channel();
        pending.lock().await.insert(11, tx);
        let task = tokio::spawn(stdout_reader_loop(
            BufReader::new(client),
            pending,
            "missing-result".to_string(),
        ));

        let body = r#"{"jsonrpc":"2.0","id":11}"#;
        let message = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
        server.write_all(message.as_bytes()).await.unwrap();

        let error = rx.await.unwrap().unwrap_err();
        assert_eq!(error.to_string(), "MCP response missing result");
        drop(server);
        let _ = task.await;
    }

    #[tokio::test]
    async fn stdout_reader_accepts_content_length_after_other_headers() {
        let (client, mut server) = duplex(1024);
        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let (tx, rx) = oneshot::channel();
        pending.lock().await.insert(12, tx);
        let task = tokio::spawn(stdout_reader_loop(
            BufReader::new(client),
            pending,
            "header-order".to_string(),
        ));

        let body = r#"{"jsonrpc":"2.0","id":12,"result":{"ok":true}}"#;
        let message = format!(
            "Content-Type: application/json\r\nContent-length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        server.write_all(message.as_bytes()).await.unwrap();

        let result = rx.await.unwrap().unwrap();
        assert_eq!(result["ok"].as_bool(), Some(true));
        drop(server);
        let _ = task.await;
    }

    #[test]
    fn validate_mcp_command_rejects_paths_and_unknown_binaries() {
        assert!(validate_mcp_command("npx").is_ok());
        assert!(validate_mcp_command("uvx").is_ok());
        assert!(validate_mcp_command("docker").is_ok());
        assert!(validate_mcp_command("../npx").is_err());
        assert!(validate_mcp_command("node").is_err());
        assert!(validate_mcp_command("python3").is_err());
        assert!(validate_mcp_command("uv").is_err());
        assert!(validate_mcp_command("bash").is_err());
        assert!(validate_mcp_command("/usr/bin/python3").is_err());
    }

    #[test]
    fn validate_mcp_env_blocks_injection_variables() {
        let mut env = HashMap::new();
        env.insert("LD_PRELOAD".to_string(), "evil.so".to_string());
        assert!(validate_mcp_env(&env).is_err());
        env.clear();
        env.insert(
            "Node_Options".to_string(),
            "--require ./evil.js".to_string(),
        );
        assert!(validate_mcp_env(&env).is_err());
        env.clear();
        env.insert("PYTHONPATH".to_string(), "/tmp/evil".to_string());
        assert!(validate_mcp_env(&env).is_err());
        env.clear();
        env.insert("PYTHONSTARTUP".to_string(), "/tmp/startup.py".to_string());
        assert!(validate_mcp_env(&env).is_err());
        env.clear();
        env.insert("SAFE".to_string(), "1".to_string());
        assert!(validate_mcp_env(&env).is_ok());
    }

    #[test]
    fn duplicate_server_names_are_disambiguated() {
        let config_a = McpServerConfig {
            id: "a".to_string(),
            name: "shared".to_string(),
            transport: McpTransport::Stdio,
            url: None,
            command: Some("npx".to_string()),
            args: Vec::new(),
            env: HashMap::new(),
            auth_header_name: None,
            auth_header_mode: None,
            headers: HashMap::new(),
            enabled: true,
            retry_on_disconnect: false,
            auth_token: None,
        };
        let mut servers = HashMap::new();
        servers.insert(
            "a".to_string(),
            McpServerState {
                config: config_a.clone(),
                status: McpServerStatus::Connected,
                error: None,
                capabilities: None,
                tools: Vec::new(),
                resources: Vec::new(),
                runtime_id: None,
                endpoint_url: None,
                resolved_transport: None,
                session_id: None,
                generation: 1,
            },
        );
        let mut config_b = config_a;
        config_b.id = "b".to_string();
        servers.insert(
            "b".to_string(),
            McpServerState {
                config: config_b,
                status: McpServerStatus::Connected,
                error: None,
                capabilities: None,
                tools: Vec::new(),
                resources: Vec::new(),
                runtime_id: None,
                endpoint_url: None,
                resolved_transport: None,
                session_id: None,
                generation: 1,
            },
        );
        assert_eq!(
            server_namespace(servers.get("a").unwrap(), &servers),
            "shared#a"
        );
    }

    #[test]
    fn mcp_resource_tools_are_exposed_without_connected_resources() {
        let registry = McpRegistry::new(AiProviderKeyStore::new());
        let names = registry
            .tool_definitions()
            .into_iter()
            .map(|tool| tool.name)
            .collect::<Vec<_>>();

        assert!(names.contains(&"list_mcp_resources".to_string()));
        assert!(names.contains(&"read_mcp_resource".to_string()));
    }

    #[test]
    fn mcp_tools_and_resources_follow_config_order() {
        let registry = McpRegistry::new(AiProviderKeyStore::new());
        let mut state = registry.state.write();
        state.server_order = vec!["b".to_string(), "a".to_string()];
        let mut server_a = connected_http_state(
            http_test_config("a", McpTransport::StreamableHttp, "http://127.0.0.1/a"),
            1,
            "tool-a",
        );
        server_a.resources = vec![McpResource {
            uri: "test://a".to_string(),
            name: "A".to_string(),
            description: None,
            mime_type: None,
        }];
        let mut server_b = connected_http_state(
            http_test_config("b", McpTransport::StreamableHttp, "http://127.0.0.1/b"),
            1,
            "tool-b",
        );
        server_b.resources = vec![McpResource {
            uri: "test://b".to_string(),
            name: "B".to_string(),
            description: None,
            mime_type: None,
        }];
        state.servers.insert("a".to_string(), server_a);
        state.servers.insert("b".to_string(), server_b);
        drop(state);

        let tool_names = registry
            .tool_definitions()
            .into_iter()
            .map(|tool| tool.name)
            .collect::<Vec<_>>();
        let dynamic_names = tool_names
            .into_iter()
            .filter(|name| name.starts_with("mcp::"))
            .collect::<Vec<_>>();
        assert_eq!(dynamic_names, vec!["mcp::b::tool-b", "mcp::a::tool-a"]);

        let resource_uris = registry
            .resources()
            .into_iter()
            .map(|(resource, _, _)| resource.uri)
            .collect::<Vec<_>>();
        assert_eq!(resource_uris, vec!["test://b", "test://a"]);
    }

    #[tokio::test]
    async fn streamable_http_server_connects_and_exposes_tools() {
        let (url, task) = spawn_streamable_http_mcp_server(false).await;
        let registry = McpRegistry::new(AiProviderKeyStore::new());
        registry
            .connect_config(http_test_config("http", McpTransport::StreamableHttp, &url))
            .await;
        let snapshots = registry.snapshots();
        let snapshot = snapshots
            .iter()
            .find(|server| server.config.id == "http")
            .unwrap();
        assert_eq!(snapshot.status, "connected");
        assert_eq!(
            snapshot.resolved_transport.as_deref(),
            Some("streamable-http")
        );
        assert_eq!(snapshot.session_id.as_deref(), Some("resources-session"));
        assert_eq!(snapshot.tools[0].name, "ping");
        assert!(
            registry
                .tool_definitions()
                .iter()
                .any(|tool| tool.name == "mcp::http::ping")
        );
        task.abort();
    }

    #[tokio::test]
    async fn connect_all_values_waits_for_enabled_connections() {
        let (url, task) = spawn_streamable_http_mcp_server(false).await;
        let registry = McpRegistry::new(AiProviderKeyStore::new());
        let config =
            serde_json::to_value(http_test_config("http", McpTransport::StreamableHttp, &url))
                .unwrap();

        registry.connect_all_values(&[config]).await;

        let snapshot = registry
            .snapshots()
            .into_iter()
            .find(|server| server.config.id == "http")
            .unwrap();
        assert_eq!(snapshot.status, "connected");
        assert_eq!(snapshot.tools[0].name, "ping");
        task.abort();
    }

    #[tokio::test]
    async fn streamable_http_falls_back_to_legacy_sse() {
        let (url, task) = spawn_streamable_http_mcp_server(true).await;
        let registry = McpRegistry::new(AiProviderKeyStore::new());
        registry
            .connect_config(http_test_config("http", McpTransport::StreamableHttp, &url))
            .await;
        let snapshots = registry.snapshots();
        let snapshot = snapshots
            .iter()
            .find(|server| server.config.id == "http")
            .unwrap();
        assert_eq!(snapshot.status, "connected");
        assert_eq!(snapshot.resolved_transport.as_deref(), Some("legacy-sse"));
        assert!(
            snapshot
                .endpoint_url
                .as_deref()
                .unwrap()
                .ends_with("/message")
        );
        task.abort();
    }

    #[tokio::test]
    async fn synchronize_disconnects_removed_servers() {
        let registry = McpRegistry::new(AiProviderKeyStore::new());
        let mut state = registry.state.write();
        state.servers.insert(
            "old".to_string(),
            McpServerState {
                config: http_test_config("old", McpTransport::StreamableHttp, "http://127.0.0.1"),
                status: McpServerStatus::Connected,
                error: None,
                capabilities: None,
                tools: Vec::new(),
                resources: Vec::new(),
                runtime_id: None,
                endpoint_url: Some("http://127.0.0.1".to_string()),
                resolved_transport: Some(McpEffectiveTransport::StreamableHttp),
                session_id: None,
                generation: 1,
            },
        );
        drop(state);
        registry.synchronize_configs(Vec::new()).await;
        assert!(registry.snapshots().is_empty());
    }

    #[tokio::test]
    async fn stale_runtime_error_does_not_clobber_new_generation() {
        let registry = McpRegistry::new(AiProviderKeyStore::new());
        {
            let mut state = registry.state.write();
            state.generations.insert("srv".to_string(), 2);
            state.servers.insert(
                "srv".to_string(),
                connected_http_state(
                    http_test_config("srv", McpTransport::StreamableHttp, "http://127.0.0.1"),
                    2,
                    "new-tool",
                ),
            );
        }

        registry
            .apply_runtime_error("srv", 1, "old socket closed".to_string())
            .await;

        let snapshot = registry.snapshots().pop().unwrap();
        assert_eq!(snapshot.status, "connected");
        assert_eq!(snapshot.tools[0].name, "new-tool");
        assert!(snapshot.error.is_none());
    }

    #[tokio::test]
    async fn runtime_error_preserves_http_transport_metadata() {
        let registry = McpRegistry::new(AiProviderKeyStore::new());
        {
            let mut state = registry.state.write();
            state.generations.insert("srv".to_string(), 1);
            let mut server = connected_http_state(
                http_test_config("srv", McpTransport::StreamableHttp, "http://127.0.0.1"),
                1,
                "ping",
            );
            server.endpoint_url = Some("http://127.0.0.1/message".to_string());
            server.session_id = Some("session-1".to_string());
            server.resolved_transport = Some(McpEffectiveTransport::LegacySse);
            state.servers.insert("srv".to_string(), server);
        }

        registry
            .apply_runtime_error("srv", 1, "socket closed".to_string())
            .await;

        let snapshot = registry.snapshots().pop().unwrap();
        assert_eq!(snapshot.status, "error");
        assert_eq!(
            snapshot.endpoint_url.as_deref(),
            Some("http://127.0.0.1/message")
        );
        assert_eq!(snapshot.session_id.as_deref(), Some("session-1"));
        assert_eq!(snapshot.resolved_transport.as_deref(), Some("legacy-sse"));
        assert!(snapshot.tools.is_empty());
    }

    #[test]
    fn validate_http_url_rejects_non_http_transports() {
        assert!(validate_mcp_http_url("http://localhost:3000").is_ok());
        assert!(validate_mcp_http_url("https://example.com/mcp").is_ok());
        assert!(validate_mcp_http_url("file:///tmp/mcp").is_err());
    }

    #[test]
    fn http_headers_match_tauri_auth_and_reserved_filtering() {
        let mut config = http_test_config("auth", McpTransport::StreamableHttp, "http://127.0.0.1");
        config.auth_header_name = Some("X-API-Key".to_string());
        config.auth_header_mode = Some(McpAuthHeaderMode::Raw);
        config
            .headers
            .insert("X-Workspace".to_string(), "prod".to_string());
        config
            .headers
            .insert("Accept".to_string(), "text/plain".to_string());
        config
            .headers
            .insert("MCP-Session-Id".to_string(), "bad".to_string());

        let headers = build_http_headers(
            &config,
            Some("token-123"),
            None,
            STREAMABLE_HTTP_PROTOCOL_VERSION,
            true,
            "application/json, text/event-stream",
        )
        .unwrap();

        assert_eq!(headers["X-API-Key"], "token-123");
        assert_eq!(headers["X-Workspace"], "prod");
        assert_eq!(headers["accept"], "application/json, text/event-stream");
        assert!(!headers.contains_key("authorization"));
        assert!(!headers.contains_key("mcp-session-id"));
    }

    #[test]
    fn sse_parser_matches_tauri_line_semantics() {
        let mut parser = SseEventParser::default();
        parser.push_str(": keepalive\r\n");
        parser.push_str("event: message\r\n");
        parser.push_str("data: {\"a\":1}\r\n");
        parser.push_str("data: {\"b\":2}\r\n\r\n");
        parser.push_str("data: tail");

        let events = parser.drain_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].name, "message");
        assert_eq!(events[0].data, "{\"a\":1}\n{\"b\":2}");

        let events = parser.finish();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].name, "message");
        assert_eq!(events[0].data, "tail");
    }

    #[test]
    fn mcp_tool_output_keeps_error_text_out_of_truncation_meta() {
        let result = McpCallToolResult {
            is_error: true,
            content: vec![McpCallContent {
                content_type: "text".to_string(),
                text: Some("bad input".to_string()),
                data: None,
                mime_type: None,
            }],
        };

        let (ok, output, truncated) = mcp_tool_output(&result);
        assert!(!ok);
        assert_eq!(output, "bad input");
        assert!(!truncated);
    }

    #[test]
    fn mcp_output_truncates_like_tauri_char_slice() {
        let text = "你".repeat(MCP_TOOL_OUTPUT_MAX_CHARS + 1);
        let result = McpCallToolResult {
            is_error: false,
            content: vec![McpCallContent {
                content_type: "text".to_string(),
                text: Some(text),
                data: None,
                mime_type: None,
            }],
        };

        let (ok, output, truncated) = mcp_tool_output(&result);
        assert!(ok);
        assert!(truncated);
        assert_eq!(output.chars().count(), MCP_TOOL_OUTPUT_MAX_CHARS);
    }

    fn http_test_config(id: &str, transport: McpTransport, url: &str) -> McpServerConfig {
        McpServerConfig {
            id: id.to_string(),
            name: id.to_string(),
            transport,
            url: Some(url.to_string()),
            command: None,
            args: Vec::new(),
            env: HashMap::new(),
            auth_header_name: None,
            auth_header_mode: None,
            headers: HashMap::new(),
            enabled: true,
            retry_on_disconnect: false,
            auth_token: None,
        }
    }

    fn connected_http_state(
        config: McpServerConfig,
        generation: u64,
        tool_name: &str,
    ) -> McpServerState {
        McpServerState {
            config,
            status: McpServerStatus::Connected,
            error: None,
            capabilities: Some(McpServerCapabilities {
                tools: Some(serde_json::json!({})),
                resources: None,
                prompts: None,
            }),
            tools: vec![McpToolSchema {
                name: tool_name.to_string(),
                description: None,
                input_schema: serde_json::json!({ "type": "object" }),
            }],
            resources: Vec::new(),
            runtime_id: None,
            endpoint_url: Some("http://127.0.0.1".to_string()),
            resolved_transport: Some(McpEffectiveTransport::StreamableHttp),
            session_id: None,
            generation,
        }
    }

    async fn spawn_streamable_http_mcp_server(
        force_legacy: bool,
    ) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                tokio::spawn(async move {
                    let mut buffer = Vec::new();
                    let header_end = loop {
                        let mut chunk = [0_u8; 1024];
                        let Ok(read) = stream.read(&mut chunk).await else {
                            return;
                        };
                        if read == 0 {
                            return;
                        }
                        buffer.extend_from_slice(&chunk[..read]);
                        if let Some(index) = find_header_end(&buffer) {
                            break index;
                        }
                    };
                    let headers = String::from_utf8_lossy(&buffer[..header_end]);
                    let content_length = headers
                        .lines()
                        .find_map(|line| {
                            line.strip_prefix("content-length:")
                                .or_else(|| line.strip_prefix("Content-Length:"))
                                .and_then(|value| value.trim().parse::<usize>().ok())
                        })
                        .unwrap_or_default();
                    let mut body = buffer[(header_end + 4)..].to_vec();
                    while body.len() < content_length {
                        let mut chunk = vec![0_u8; content_length - body.len()];
                        let Ok(read) = stream.read(&mut chunk).await else {
                            return;
                        };
                        if read == 0 {
                            return;
                        }
                        body.extend_from_slice(&chunk[..read]);
                    }
                    let request_line = headers.lines().next().unwrap_or_default();
                    if request_line.starts_with("GET ") {
                        let body = "event: endpoint\ndata: /message\n\n";
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        let _ = stream.write_all(response.as_bytes()).await;
                        return;
                    }
                    if force_legacy && request_line.starts_with("POST / ") {
                        let response = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
                        let _ = stream.write_all(response.as_bytes()).await;
                        return;
                    }
                    let request: Value = serde_json::from_slice(&body).unwrap_or_default();
                    let session_id = match request.get("method").and_then(Value::as_str) {
                        Some("tools/list") => "tools-session",
                        Some("resources/list") => "resources-session",
                        _ => "test-session",
                    };
                    let response_body = mcp_http_response_body(&request);
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nMCP-Session-Id: {}\r\nContent-Length: {}\r\n\r\n{}",
                        session_id,
                        response_body.len(),
                        response_body
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                });
            }
        });
        (format!("http://{addr}"), task)
    }

    fn mcp_http_response_body(request: &Value) -> String {
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let result = match request
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "initialize" => serde_json::json!({
                "protocolVersion": STREAMABLE_HTTP_PROTOCOL_VERSION,
                "capabilities": { "tools": {}, "resources": {} }
            }),
            "tools/list" => serde_json::json!({
                "tools": [{
                    "name": "ping",
                    "description": "Ping test tool",
                    "inputSchema": { "type": "object", "properties": {} }
                }]
            }),
            "resources/list" => serde_json::json!({
                "resources": [{
                    "uri": "test://resource",
                    "name": "resource",
                    "description": "Test resource",
                    "mimeType": "text/plain"
                }]
            }),
            _ => serde_json::json!({}),
        };
        serde_json::json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string()
    }

    fn find_header_end(buffer: &[u8]) -> Option<usize> {
        buffer.windows(4).position(|window| window == b"\r\n\r\n")
    }
}
