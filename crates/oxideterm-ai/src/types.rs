use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::policy::{AiPolicySafetyMode, AiToolUsePolicy};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AiProviderTemplate {
    pub provider_type: &'static str,
    pub label_key: &'static str,
    pub base_url: &'static str,
    pub default_model: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AiProviderView {
    pub id: String,
    pub provider_type: String,
    pub name: String,
    pub base_url: String,
    pub default_model: String,
    pub models: Vec<String>,
    pub enabled: bool,
    pub custom: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProviderModelRefresh {
    pub models: Vec<String>,
    pub context_windows: HashMap<String, i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModelSelectorProviderProbe {
    Disabled,
    ImplicitKey { endpoint: Option<&'static str> },
    StoredKey,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelSelectorProviderGroup {
    pub provider: AiProviderView,
    pub visible_models: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiChatRole {
    User,
    Assistant,
    System,
    Tool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AiChatMessage {
    pub id: String,
    pub role: AiChatRole,
    pub content: String,
    pub timestamp_ms: i64,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub context: Option<String>,
    #[serde(default)]
    pub thinking_content: Option<String>,
    #[serde(default)]
    pub is_streaming: bool,
    #[serde(default)]
    pub metadata: Option<AiChatMessageMetadata>,
    #[serde(default)]
    pub tool_call_id: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<serde_json::Value>,
    #[serde(default)]
    pub turn: Option<serde_json::Value>,
    #[serde(default)]
    pub transcript_ref: Option<serde_json::Value>,
    #[serde(default)]
    pub summary_ref: Option<serde_json::Value>,
    #[serde(default)]
    pub branches: Option<AiMessageBranches>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiMessageBranches {
    pub total: usize,
    pub active_index: usize,
    #[serde(default)]
    pub tails: HashMap<usize, Vec<AiChatMessage>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiChatMessageMetadata {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub original_count: Option<usize>,
    #[serde(default, rename = "compactedAt")]
    pub compacted_at_ms: Option<i64>,
    #[serde(default)]
    pub original_messages: Option<Vec<AiChatMessage>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AiConversation {
    pub id: String,
    pub title: String,
    pub messages: Vec<AiChatMessage>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub origin: String,
    pub profile_id: Option<String>,
    #[serde(default)]
    pub message_count: usize,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub session_metadata: Option<serde_json::Value>,
    #[serde(default = "default_messages_loaded")]
    pub messages_loaded: bool,
}

fn default_messages_loaded() -> bool {
    true
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct AiChatState {
    #[serde(default)]
    pub conversations: Vec<AiConversation>,
    #[serde(default)]
    pub active_conversation_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

impl AiToolCall {
    pub fn from_value(value: &serde_json::Value) -> Option<Self> {
        let id = value.get("id").and_then(serde_json::Value::as_str)?;
        let name = value
            .get("name")
            .and_then(serde_json::Value::as_str)
            .or_else(|| {
                value
                    .get("function")
                    .and_then(|function| function.get("name"))
                    .and_then(serde_json::Value::as_str)
            })?;
        let arguments = value
            .get("arguments")
            .and_then(serde_json::Value::as_str)
            .or_else(|| {
                value
                    .get("function")
                    .and_then(|function| function.get("arguments"))
                    .and_then(serde_json::Value::as_str)
            })
            .unwrap_or_default();
        Some(Self {
            id: id.to_string(),
            name: name.to_string(),
            arguments: arguments.to_string(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AiToolChoice {
    #[default]
    Auto,
    Required,
    Named(String),
}

#[derive(Clone)]
pub struct AiChatStreamConfig {
    pub provider_id: Option<String>,
    pub provider_type: String,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<Zeroizing<String>>,
    pub max_response_tokens: Option<i64>,
    pub reasoning_effort: Option<String>,
    pub safety_mode: AiPolicySafetyMode,
    pub profile_id: Option<String>,
    pub tool_policy: AiToolUsePolicy,
    pub tools: Vec<AiToolDefinition>,
    pub tool_choice: AiToolChoice,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AiStreamEvent {
    Content(String),
    Thinking(String),
    ToolCall {
        id: String,
        name: String,
        arguments: String,
    },
    ToolCallComplete {
        id: String,
        name: String,
        arguments: String,
    },
    Done,
    Error(String),
}
