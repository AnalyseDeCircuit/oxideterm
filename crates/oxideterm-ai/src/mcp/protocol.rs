const MODERN_PROTOCOL_VERSION: &str = "2026-07-28";
const LEGACY_STREAMABLE_HTTP_PROTOCOL_VERSION: &str = "2025-11-25";
const LEGACY_SSE_PROTOCOL_VERSION: &str = "2024-11-05";
const MCP_TASKS_EXTENSION: &str = "io.modelcontextprotocol/tasks";
const MCP_HEADER_MISMATCH: i64 = -32020;
const MCP_MISSING_REQUIRED_CLIENT_CAPABILITY: i64 = -32021;
const MCP_UNSUPPORTED_PROTOCOL_VERSION: i64 = -32022;
const JSON_RPC_METHOD_NOT_FOUND: i64 = -32601;

type McpHttpNegotiation = (
    McpProtocol,
    McpEffectiveTransport,
    String,
    Option<String>,
    McpServerCapabilities,
);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum McpProtocolEra {
    Modern,
    Legacy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum McpStdioFraming {
    LineDelimited,
    LegacyContentLength,
}

impl McpProtocolEra {
    fn as_str(self) -> &'static str {
        match self {
            Self::Modern => "modern",
            Self::Legacy => "legacy",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct McpProtocol {
    era: McpProtocolEra,
    version: String,
    stdio_framing: McpStdioFraming,
}

impl McpProtocol {
    fn modern(version: impl Into<String>) -> Self {
        Self {
            era: McpProtocolEra::Modern,
            version: version.into(),
            stdio_framing: McpStdioFraming::LineDelimited,
        }
    }

    fn legacy(version: impl Into<String>) -> Self {
        Self {
            era: McpProtocolEra::Legacy,
            version: version.into(),
            stdio_framing: McpStdioFraming::LineDelimited,
        }
    }

    fn modern_preferred() -> Self {
        Self::modern(MODERN_PROTOCOL_VERSION)
    }

    fn legacy_streamable_http() -> Self {
        Self::legacy(LEGACY_STREAMABLE_HTTP_PROTOCOL_VERSION)
    }

    fn legacy_content_length_stdio() -> Self {
        Self {
            era: McpProtocolEra::Legacy,
            version: LEGACY_STREAMABLE_HTTP_PROTOCOL_VERSION.to_string(),
            stdio_framing: McpStdioFraming::LegacyContentLength,
        }
    }

    fn legacy_sse() -> Self {
        Self::legacy(LEGACY_SSE_PROTOCOL_VERSION)
    }

    fn is_modern(&self) -> bool {
        self.era == McpProtocolEra::Modern
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct McpDiscoverResult {
    supported_versions: Vec<String>,
    capabilities: McpServerCapabilities,
    #[serde(default)]
    #[serde(rename = "instructions")]
    _instructions: Option<String>,
    #[serde(flatten)]
    _cache: McpCacheHint,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct McpCacheHint {
    #[serde(default)]
    ttl_ms: u64,
    #[serde(default)]
    #[serde(rename = "cacheScope")]
    _cache_scope: McpCacheScope,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
enum McpCacheScope {
    Public,
    #[default]
    Private,
}

#[derive(Clone, Debug)]
struct McpCachedResult<T> {
    value: T,
    hint: McpCacheHint,
    received_at: std::time::Instant,
    session_id: Option<String>,
}

impl<T> McpCachedResult<T> {
    fn new(value: T, hint: McpCacheHint) -> Self {
        Self {
            value,
            hint,
            received_at: std::time::Instant::now(),
            session_id: None,
        }
    }

    fn with_session_id(mut self, session_id: Option<String>) -> Self {
        self.session_id = session_id;
        self
    }

    fn is_fresh(&self) -> bool {
        self.hint.ttl_ms > 0 && self.received_at.elapsed() < Duration::from_millis(self.hint.ttl_ms)
    }
}

#[derive(Clone, Debug)]
enum McpResultEnvelope {
    Complete(Value),
    InputRequired(McpInputRequiredResult),
    Task(Box<McpTask>),
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct McpInputRequiredResult {
    #[serde(default)]
    input_requests: serde_json::Map<String, Value>,
    request_state: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct McpTask {
    task_id: String,
    status: McpTaskStatus,
    #[serde(default)]
    status_message: Option<String>,
    #[serde(default)]
    ttl_ms: Option<u64>,
    #[serde(default)]
    poll_interval_ms: Option<u64>,
    #[serde(default)]
    input_requests: serde_json::Map<String, Value>,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<Value>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum McpTaskStatus {
    Working,
    InputRequired,
    Completed,
    Failed,
    Cancelled,
}

struct McpTaskCancellationGuard {
    registry: McpRegistry,
    server_id: String,
    generation: u64,
    task_id: String,
    armed: bool,
}

impl McpTaskCancellationGuard {
    fn new(registry: McpRegistry, server: &McpServerState, task_id: &str) -> Self {
        Self {
            registry,
            server_id: server.config.id.clone(),
            generation: server.generation,
            task_id: task_id.to_string(),
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for McpTaskCancellationGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let registry = self.registry.clone();
        let server_id = self.server_id.clone();
        let generation = self.generation;
        let task_id = self.task_id.clone();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                let Ok(server) = registry.connected_server(&server_id) else {
                    return;
                };
                if server.generation != generation {
                    return;
                }
                let _ = registry
                    .rpc_result_round(
                        &server,
                        "tasks/cancel",
                        Some(serde_json::json!({ "taskId": task_id })),
                        None,
                        None,
                    )
                    .await;
            });
        }
    }
}

fn modern_client_capabilities() -> Value {
    // Tasks are advertised because the registry implements polling and task cancellation.
    serde_json::json!({
        "extensions": {
            MCP_TASKS_EXTENSION: {}
        }
    })
}

fn request_params_for_protocol(
    protocol: &McpProtocol,
    params: Option<Value>,
    input_responses: Option<Value>,
    request_state: Option<&str>,
) -> Result<Value, McpError> {
    let mut params = match params.unwrap_or_else(|| serde_json::json!({})) {
        Value::Object(params) => params,
        _ => {
            return Err(McpError::Message(
                "MCP request params must be a JSON object".to_string(),
            ));
        }
    };
    if protocol.is_modern() {
        params.insert(
            "_meta".to_string(),
            serde_json::json!({
                "io.modelcontextprotocol/protocolVersion": protocol.version,
                "io.modelcontextprotocol/clientInfo": {
                    "name": MCP_CLIENT_NAME,
                    "version": MCP_CLIENT_VERSION,
                },
                "io.modelcontextprotocol/clientCapabilities": modern_client_capabilities(),
            }),
        );
        if let Some(input_responses) = input_responses {
            params.insert("inputResponses".to_string(), input_responses);
        }
        if let Some(request_state) = request_state {
            params.insert(
                "requestState".to_string(),
                Value::String(request_state.to_string()),
            );
        }
    }
    Ok(Value::Object(params))
}

fn json_rpc_request_for_protocol(
    protocol: &McpProtocol,
    method: &str,
    params: Option<Value>,
) -> Result<Value, McpError> {
    Ok(json_rpc_request(
        method,
        Some(request_params_for_protocol(protocol, params, None, None)?),
    ))
}

fn parse_result_for_protocol(
    protocol: &McpProtocol,
    result: Value,
) -> Result<McpResultEnvelope, McpError> {
    if !protocol.is_modern() {
        return Ok(McpResultEnvelope::Complete(result));
    }
    match result
        .get("resultType")
        .and_then(Value::as_str)
        .unwrap_or_default()
    {
        "complete" => Ok(McpResultEnvelope::Complete(result)),
        "input_required" => serde_json::from_value(result)
            .map(McpResultEnvelope::InputRequired)
            .map_err(|error| McpError::Message(error.to_string())),
        "task" => serde_json::from_value(result)
            .map(Box::new)
            .map(McpResultEnvelope::Task)
            .map_err(|error| McpError::Message(error.to_string())),
        "" => Err(McpError::Message(
            "Modern MCP response is missing resultType".to_string(),
        )),
        other => Err(McpError::Message(format!(
            "Unsupported MCP resultType: {other}"
        ))),
    }
}

fn complete_result(server: &McpServerState, result: Value) -> Result<Value, McpError> {
    let protocol = server
        .protocol
        .as_ref()
        .ok_or_else(|| McpError::Message("MCP protocol was not negotiated".to_string()))?;
    match parse_result_for_protocol(protocol, result)? {
        McpResultEnvelope::Complete(result) => Ok(result),
        McpResultEnvelope::InputRequired(_) => Err(McpError::Message(
            "MCP list/read request unexpectedly requires client input".to_string(),
        )),
        McpResultEnvelope::Task(_) => Err(McpError::Message(
            "MCP list/read request unexpectedly returned a task".to_string(),
        )),
    }
}

fn parse_cache_hint(result: &Value) -> McpCacheHint {
    serde_json::from_value(result.clone()).unwrap_or_default()
}

fn merge_cache_hint(current: Option<McpCacheHint>, next: McpCacheHint) -> McpCacheHint {
    let Some(current) = current else {
        return next;
    };
    McpCacheHint {
        ttl_ms: current.ttl_ms.min(next.ttl_ms),
        _cache_scope: if current._cache_scope == McpCacheScope::Private
            || next._cache_scope == McpCacheScope::Private
        {
            McpCacheScope::Private
        } else {
            McpCacheScope::Public
        },
    }
}

fn select_supported_modern_version(supported: &[String]) -> Option<McpProtocol> {
    supported
        .iter()
        .find(|version| version.as_str() == MODERN_PROTOCOL_VERSION)
        .cloned()
        .map(McpProtocol::modern)
}

fn is_recognized_modern_error(error: &McpError) -> bool {
    matches!(
        error,
        McpError::Rpc {
            code: MCP_HEADER_MISMATCH
                | MCP_MISSING_REQUIRED_CLIENT_CAPABILITY
                | MCP_UNSUPPORTED_PROTOCOL_VERSION,
            ..
        }
    )
}

fn supported_versions_from_error(error: &McpError) -> Vec<String> {
    let McpError::Rpc {
        code: MCP_UNSUPPORTED_PROTOCOL_VERSION,
        data: Some(data),
        ..
    } = error
    else {
        return Vec::new();
    };
    data.get("supported")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

fn rpc_error_from_value(error: &Value) -> McpError {
    rpc_error_from_value_with_status(error, None)
}

fn rpc_error_from_value_with_status(error: &Value, status: Option<StatusCode>) -> McpError {
    McpError::Rpc {
        code: error
            .get("code")
            .and_then(Value::as_i64)
            .unwrap_or_default(),
        message: error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("Unknown MCP error")
            .to_string(),
        data: error.get("data").cloned(),
        status,
    }
}

fn is_recognized_modern_http_error(error: &McpError) -> bool {
    is_recognized_modern_error(error)
        || matches!(
            error,
            McpError::Rpc {
                code: JSON_RPC_METHOD_NOT_FOUND,
                status: Some(StatusCode::NOT_FOUND),
                ..
            }
        )
}

fn is_legacy_http_probe_error(error: &McpError) -> bool {
    if is_recognized_modern_http_error(error) {
        return false;
    }
    match error {
        McpError::HttpStatus(status, _) => matches!(
            *status,
            StatusCode::BAD_REQUEST | StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED
        ),
        McpError::Rpc {
            status: Some(status),
            ..
        } => matches!(
            *status,
            StatusCode::BAD_REQUEST | StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED
        ),
        _ => false,
    }
}

fn subscription_filters(server: &McpServerState) -> Option<Value> {
    let capabilities = server.capabilities.as_ref()?;
    let tools_changed = capabilities
        .tools
        .as_ref()
        .and_then(|tools| tools.get("listChanged"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let resources_changed = capabilities
        .resources
        .as_ref()
        .and_then(|resources| resources.get("listChanged"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let resources_subscribable = capabilities
        .resources
        .as_ref()
        .and_then(|resources| resources.get("subscribe"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !tools_changed
        && !resources_changed
        && (!resources_subscribable || server.resource_subscriptions.is_empty())
    {
        return None;
    }
    let mut filters = serde_json::Map::new();
    if tools_changed {
        filters.insert("toolsListChanged".to_string(), Value::Bool(true));
    }
    if resources_changed {
        filters.insert("resourcesListChanged".to_string(), Value::Bool(true));
    }
    if resources_subscribable && !server.resource_subscriptions.is_empty() {
        let mut uris = server
            .resource_subscriptions
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        uris.sort();
        filters.insert("resourceSubscriptions".to_string(), serde_json::json!(uris));
    }
    Some(Value::Object(filters))
}

fn server_supports_tasks(server: &McpServerState) -> bool {
    server
        .capabilities
        .as_ref()
        .and_then(|capabilities| capabilities.extensions.as_ref())
        .is_some_and(|extensions| extensions.contains_key(MCP_TASKS_EXTENSION))
}

fn notification_matches_subscription(notification: &Value, subscription_id: &Value) -> bool {
    notification.pointer("/params/_meta/io.modelcontextprotocol~1subscriptionId")
        == Some(subscription_id)
}

fn acknowledged_subscription_filter(notification: &Value) -> Result<Value, McpError> {
    notification
        .pointer("/params/notifications")
        .filter(|filter| filter.is_object())
        .cloned()
        .ok_or_else(|| {
            McpError::Message("MCP subscription acknowledgment is missing its filter".to_string())
        })
}

fn subscription_filter_allows_notification(filter: &Value, notification: &Value) -> bool {
    match notification.get("method").and_then(Value::as_str) {
        Some("notifications/tools/list_changed") => filter
            .get("toolsListChanged")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        Some("notifications/resources/list_changed") => filter
            .get("resourcesListChanged")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        Some("notifications/resources/updated") => {
            let Some(uri) = notification.pointer("/params/uri").and_then(Value::as_str) else {
                return false;
            };
            filter
                .get("resourceSubscriptions")
                .and_then(Value::as_array)
                .is_some_and(|uris| uris.iter().any(|value| value.as_str() == Some(uri)))
        }
        _ => false,
    }
}
