use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

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
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AiChatMessage {
    pub id: String,
    pub role: AiChatRole,
    pub content: String,
    pub timestamp_ms: i64,
    pub model: Option<String>,
    pub context: Option<String>,
    pub is_streaming: bool,
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
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct AiChatState {
    #[serde(default)]
    pub conversations: Vec<AiConversation>,
    #[serde(default)]
    pub active_conversation_id: Option<String>,
}

pub struct AiChatStreamConfig {
    pub provider_type: String,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<Zeroizing<String>>,
    pub max_response_tokens: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AiStreamEvent {
    Content(String),
    Thinking(String),
    Done,
    Error(String),
}
