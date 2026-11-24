use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, fmt};

const MCP_TOOL_OUTPUT_MAX_CHARS: usize = 8_192;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum McpTransport {
    Stdio,
    StreamableHttp,
    LegacySse,
    Sse,
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

#[derive(Clone, Deserialize, Serialize)]
pub struct McpCallContent {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: Option<String>,
    pub data: Option<String>,
    #[serde(rename = "mimeType", alias = "mime_type")]
    pub mime_type: Option<String>,
}

impl std::fmt::Debug for McpCallContent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("McpCallContent")
            .field("content_type", &self.content_type)
            .field("mime_type", &self.mime_type)
            .finish_non_exhaustive()
    }
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
