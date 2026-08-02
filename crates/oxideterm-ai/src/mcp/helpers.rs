fn validate_mcp_command(command: &str) -> Result<(), McpError> {
    if command.trim().is_empty() {
        return Err(McpError::Message(
            "MCP command must not be empty".to_string(),
        ));
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
    protocol: &McpProtocol,
    content_type: bool,
    accept: &str,
) -> Result<reqwest::header::HeaderMap, McpError> {
    const RESERVED: &[&str] = &[
        "accept",
        "content-type",
        "mcp-session-id",
        "mcp-protocol-version",
        "mcp-method",
        "mcp-name",
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
        // HeaderMap owns the bytes needed for the outgoing request, but the
        // temporary formatted auth value should still be wiped after parsing.
        let value = Zeroizing::new(
            match config.auth_header_mode.unwrap_or(McpAuthHeaderMode::Bearer) {
                McpAuthHeaderMode::Bearer => format!("Bearer {token}"),
                McpAuthHeaderMode::Raw => token.to_string(),
                McpAuthHeaderMode::None => String::new(),
            },
        );
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
        protocol.version.parse().expect("valid protocol version"),
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

fn insert_modern_request_headers(
    headers: &mut reqwest::header::HeaderMap,
    request: &Value,
    tool_schema: Option<&McpToolSchema>,
) -> Result<(), McpError> {
    let method = request
        .get("method")
        .and_then(Value::as_str)
        .ok_or_else(|| McpError::Message("MCP request is missing method".to_string()))?;
    headers.insert(
        HeaderName::from_static("mcp-method"),
        method
            .parse()
            .map_err(|_| McpError::Message("Invalid MCP method header".to_string()))?,
    );
    let params = request.get("params").and_then(Value::as_object);
    let name = match method {
        "tools/call" | "prompts/get" => params
            .and_then(|params| params.get("name"))
            .and_then(Value::as_str),
        "resources/read" => params
            .and_then(|params| params.get("uri"))
            .and_then(Value::as_str),
        _ => None,
    };
    if matches!(method, "tools/call" | "prompts/get" | "resources/read") {
        let name = name.ok_or_else(|| {
            McpError::Message(format!("MCP request {method} is missing its routing name"))
        })?;
        headers.insert(
            HeaderName::from_static("mcp-name"),
            encode_mcp_header_value(name)
                .parse()
                .map_err(|_| McpError::Message("Invalid MCP name header".to_string()))?,
        );
    }

    if method == "tools/call"
        && let Some(tool_schema) = tool_schema
    {
        let bindings = mcp_tool_header_bindings(&tool_schema.input_schema)?;
        let arguments = params
            .and_then(|params| params.get("arguments"))
            .unwrap_or(&Value::Null);
        for binding in bindings {
            let Some(value) = value_at_path(arguments, &binding.path) else {
                continue;
            };
            if value.is_null() {
                continue;
            }
            let value = primitive_mcp_header_value(value).map_err(|reason| {
                McpError::Message(format!(
                    "MCP tool {} supplied an invalid header parameter: {reason}",
                    tool_schema.name
                ))
            })?;
            let header_name =
                HeaderName::from_bytes(format!("mcp-param-{}", binding.header_name).as_bytes())
                    .map_err(|_| {
                        McpError::Message(format!(
                            "Invalid x-mcp-header on MCP tool {}",
                            tool_schema.name
                        ))
                    })?;
            headers.insert(
                header_name,
                encode_mcp_header_value(&value).parse().map_err(|_| {
                    McpError::Message(format!(
                        "Invalid MCP parameter header on tool {}",
                        tool_schema.name
                    ))
                })?,
            );
        }
    }
    Ok(())
}

fn encode_mcp_header_value(value: &str) -> String {
    let plain_ascii = !value.is_empty()
        && value.trim() == value
        && value
            .as_bytes()
            .iter()
            .all(|byte| matches!(*byte, b'\t' | 0x20..=0x7e))
        && !(value.starts_with("=?base64?") && value.ends_with("?="));
    if plain_ascii {
        value.to_string()
    } else {
        format!(
            "=?base64?{}?=",
            base64::engine::general_purpose::STANDARD.encode(value.as_bytes())
        )
    }
}

fn primitive_mcp_header_value(value: &Value) -> Result<String, &'static str> {
    const MAX_SAFE_INTEGER: u64 = (1_u64 << 53) - 1;
    match value {
        Value::String(value) => Ok(value.clone()),
        Value::Bool(value) => Ok(value.to_string()),
        Value::Number(value) => {
            let is_safe = value
                .as_i64()
                .is_some_and(|value| value.unsigned_abs() <= MAX_SAFE_INTEGER)
                || value
                    .as_u64()
                    .is_some_and(|value| value <= MAX_SAFE_INTEGER);
            is_safe
                .then(|| value.to_string())
                .ok_or("integer is outside the JavaScript safe range")
        }
        _ => Err("value is not a string, integer, or boolean"),
    }
}

#[derive(Debug)]
struct McpHeaderBinding {
    path: Vec<String>,
    header_name: String,
}

fn mcp_tool_header_bindings(schema: &Value) -> Result<Vec<McpHeaderBinding>, McpError> {
    let mut bindings = Vec::new();
    let mut names = std::collections::HashSet::new();
    inspect_mcp_header_schema(schema, &[], false, &mut bindings, &mut names)?;
    Ok(bindings)
}

fn inspect_mcp_header_schema(
    schema: &Value,
    path: &[String],
    annotation_allowed: bool,
    bindings: &mut Vec<McpHeaderBinding>,
    names: &mut std::collections::HashSet<String>,
) -> Result<(), McpError> {
    let Some(object) = schema.as_object() else {
        return Ok(());
    };
    if let Some(annotation) = object.get("x-mcp-header") {
        if !annotation_allowed {
            return Err(McpError::Message(
                "x-mcp-header is not on a statically reachable property".to_string(),
            ));
        }
        let header_name = annotation
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| McpError::Message("x-mcp-header must be a string".to_string()))?;
        HeaderName::from_bytes(header_name.as_bytes())
            .map_err(|_| McpError::Message("x-mcp-header is not a valid token".to_string()))?;
        if !matches!(
            object.get("type").and_then(Value::as_str),
            Some("string" | "integer" | "boolean")
        ) {
            return Err(McpError::Message(
                "x-mcp-header requires string, integer, or boolean type".to_string(),
            ));
        }
        if !names.insert(header_name.to_ascii_lowercase()) {
            return Err(McpError::Message(
                "x-mcp-header names must be unique".to_string(),
            ));
        }
        bindings.push(McpHeaderBinding {
            path: path.to_vec(),
            header_name: header_name.to_string(),
        });
    }
    for (key, child) in object {
        if key == "properties" {
            if let Some(properties) = child.as_object() {
                for (property_name, property_schema) in properties {
                    let mut child_path = path.to_vec();
                    child_path.push(property_name.clone());
                    inspect_mcp_header_schema(property_schema, &child_path, true, bindings, names)?;
                }
            }
        } else if key != "x-mcp-header" {
            match child {
                Value::Array(values) => {
                    for value in values {
                        inspect_mcp_header_schema(value, path, false, bindings, names)?;
                    }
                }
                Value::Object(_) => {
                    inspect_mcp_header_schema(child, path, false, bindings, names)?;
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn value_at_path<'a>(value: &'a Value, path: &[String]) -> Option<&'a Value> {
    path.iter()
        .try_fold(value, |current, segment| current.get(segment))
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
    let status = response.status();
    if !expect_json || matches!(status, StatusCode::ACCEPTED | StatusCode::NO_CONTENT) {
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
    match serde_json::from_str(&text) {
        Ok(value) => Ok(Some(value)),
        // A legacy endpoint may return an HTML or plain-text 4xx response to
        // the modern probe. Preserve the status so era negotiation can apply
        // the transport-defined fallback instead of surfacing a JSON error.
        Err(_) if !status.is_success() => Ok(None),
        Err(error) => Err(McpError::Message(error.to_string())),
    }
}

fn extract_result(response: Option<Value>) -> Result<Value, McpError> {
    let Some(response) = response else {
        return Err(McpError::Message("MCP response missing result".to_string()));
    };
    if let Some(error) = response.get("error") {
        return Err(rpc_error_from_value(error));
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

            let trimmed = arg.trim();
            let lower = trimmed.to_ascii_lowercase();
            let sensitive_name = lower
                .trim_start_matches('-')
                .split_once('=')
                .map(|(name, _)| name)
                .unwrap_or_else(|| lower.trim_start_matches('-'));
            let is_sensitive_flag = matches!(
                sensitive_name,
                "apikey"
                    | "api-key"
                    | "api_key"
                    | "auth"
                    | "authorization"
                    | "bearer"
                    | "password"
                    | "secret"
                    | "token"
            );

            if is_sensitive_flag && trimmed.contains('=') {
                trimmed
                    .split_once('=')
                    .map(|(name, _)| format!("{name}=[redacted]"))
                    .unwrap_or_else(|| trimmed.to_string())
            } else if is_sensitive_flag {
                redact_next = true;
                trimmed.to_string()
            } else {
                trimmed.to_string()
            }
        })
        .collect()
}

pub fn is_mcp_tool_name(name: &str) -> bool {
    name.starts_with("mcp::")
}

pub fn mcp_tool_output(result: &McpCallToolResult) -> (bool, String, bool) {
    let mut parts = result
        .content
        .iter()
        .filter(|content| content.content_type == "text")
        .filter_map(|content| content.text.as_deref())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if let Some(structured_content) = &result.structured_content {
        parts.push(
            serde_json::to_string_pretty(structured_content)
                .unwrap_or_else(|_| structured_content.to_string()),
        );
    }
    let text = parts.join("\n");
    let truncated = !result.is_error && text.chars().count() > MCP_TOOL_OUTPUT_MAX_CHARS;
    let output = truncate_chars(&text, MCP_TOOL_OUTPUT_MAX_CHARS);
    (!result.is_error, output, truncated)
}

pub fn mcp_resource_output(contents: &[McpResourceContent]) -> (String, bool) {
    let multiple = contents.len() > 1;
    let text = contents
        .iter()
        .map(|content| {
            let body = content.text.clone().unwrap_or_else(|| {
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
            if multiple {
                format!("[{}]\n{body}", content.uri)
            } else {
                body
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let truncated = text.chars().count() > MCP_TOOL_OUTPUT_MAX_CHARS;
    (truncate_chars(&text, MCP_TOOL_OUTPUT_MAX_CHARS), truncated)
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}
