use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{AiChatMessage, AiChatRole, AiConversation, stream_state::ai_tool_call_is_unfinished};

pub const HISTORY_PAGE_SIZE: usize = 50;
pub const HISTORY_CACHE_BYTES: usize = 64 * 1024 * 1024;
pub const CONTENT_CHUNK_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConversationHead {
    pub conversation: AiConversation,
    pub revision: u64,
    pub metadata_revision: u64,
    pub title_revision: u64,
    pub structure_revision: u64,
    pub active_branch: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct HistoryCursor {
    pub conversation_id: String,
    pub branch_id: String,
    pub revision: u64,
    pub before_sequence: u64,
}

#[derive(Clone, Debug)]
pub struct HistoryPage {
    pub messages: Vec<AiChatMessage>,
    pub before: Option<HistoryCursor>,
    pub revision: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MessageDescriptor {
    pub id: String,
    pub storage_id: String,
    pub sequence: u64,
    pub role: AiChatRole,
    pub timestamp_ms: i64,
    pub preview: String,
    pub turn_count: usize,
    pub history_count: usize,
    pub revision: u64,
}

#[derive(Clone, Debug)]
pub struct MessagePage {
    pub messages: Vec<MessageDescriptor>,
    pub before: Option<HistoryCursor>,
    pub after: Option<HistoryCursor>,
    pub revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct HistoryEventLocation {
    pub family: String,
    pub id: String,
    pub sequence: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct HistoryContentCursor {
    pub event: Option<HistoryEventLocation>,
    pub conversation_id: String,
    pub storage_id: String,
    pub revision: u64,
    pub path: Vec<String>,
    pub offset: u64,
    pub byte_offset: usize,
    pub after_key: Option<String>,
}

pub struct HistoryContentPage {
    pub value: Value,
    pub more: Vec<HistoryContentCursor>,
}

pub struct HistoryMessageView {
    pub message: AiChatMessage,
    pub first_section: u64,
    pub section: u64,
    pub sections: u64,
    pub more: Vec<HistoryContentCursor>,
}

impl HistoryMessageView {
    // Each activity has its own 64 KiB payload window; old rounds stay on disk.
    pub const ACTIVITY_WINDOW: u64 = 16;
}

pub fn agent_history_branch(run: &crate::agent::AgentRunId) -> String {
    format!("agent:{run}")
}

pub struct AgentCommunicationPage {
    pub message: crate::agent::AgentMessage,
    pub sequence: u64,
    pub before: Option<u64>,
    pub more: Vec<HistoryContentCursor>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersistedTranscriptEntry {
    pub id: String,
    pub conversation_id: String,
    #[serde(default)]
    pub turn_id: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
    pub timestamp: i64,
    pub kind: String,
    #[serde(default)]
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersistedDiagnosticEvent {
    pub id: String,
    pub conversation_id: String,
    #[serde(default)]
    pub turn_id: Option<String>,
    #[serde(default)]
    pub round_id: Option<String>,
    pub timestamp: i64,
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(default)]
    pub data: Value,
}

pub(crate) fn normalize_interrupted_assistant_projection(message: &mut AiChatMessage) {
    if message.role != AiChatRole::Assistant {
        return;
    }
    if let Some(turn) = message.turn.as_mut() {
        if turn.get("status").and_then(Value::as_str) == Some("streaming")
            && let Some(object) = turn.as_object_mut()
        {
            object.insert("status".to_string(), Value::String("complete".to_string()));
        }
        if let Some(parts) = turn.get_mut("parts").and_then(Value::as_array_mut) {
            for part in parts {
                if part.get("type").and_then(Value::as_str) == Some("thinking")
                    && let Some(object) = part.as_object_mut()
                {
                    object.insert("streaming".to_string(), Value::Bool(false));
                }
            }
        }
    }
    let mut rejected_tool_ids = HashSet::new();
    for call in &mut message.tool_calls {
        if ai_tool_call_is_unfinished(call) {
            let id = call
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_default();
            let result = interrupted_tool_result_payload(call);
            if let Some(object) = call.as_object_mut() {
                object.insert("status".to_string(), Value::String("rejected".to_string()));
                object.insert(
                    "summary".to_string(),
                    Value::String("Generation was stopped.".to_string()),
                );
                object.insert("result".to_string(), result);
            }
            if !id.is_empty() {
                rejected_tool_ids.insert(id);
            }
        }
    }
    if rejected_tool_ids.is_empty() {
        return;
    }
    if let Some(rounds) = message
        .turn
        .as_mut()
        .and_then(|turn| turn.get_mut("toolRounds"))
        .and_then(Value::as_array_mut)
    {
        for round in rounds {
            let Some(tool_calls) = round.get_mut("toolCalls").and_then(Value::as_array_mut) else {
                continue;
            };
            for tool_call in tool_calls {
                let id = tool_call
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if !rejected_tool_ids.contains(id) {
                    continue;
                }
                if let Some(object) = tool_call.as_object_mut() {
                    object.insert(
                        "approvalState".to_string(),
                        Value::String("rejected".to_string()),
                    );
                    object.insert(
                        "executionState".to_string(),
                        Value::String("error".to_string()),
                    );
                }
            }
        }
    }
}

fn interrupted_tool_result_payload(call: &Value) -> Value {
    let tool_name = call
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("tool")
        .to_string();
    serde_json::json!({
        "ok": false,
        "summary": "Generation was stopped.",
        "output": "Generation was stopped.",
        "data": Value::Null,
        "error": {
            "code": "generation_stopped",
            "message": "Generation was stopped.",
            "recoverable": true,
        },
        "targets": [],
        "meta": {
            "toolName": tool_name,
            "durationMs": 0,
            "verified": false,
            "capability": Value::Null,
            "truncated": false,
        }
    })
}
