//! Conversation mutations and sanitized streaming deltas shared by storage hosts.
use crate::{AiChatMessage, AiChatRole, AiConversation};
use serde::{
    Deserialize, Serialize,
    ser::{SerializeMap, SerializeSeq},
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use zeroize::Zeroizing;
#[path = "persistence/stream_delta/fields.rs"]
mod fields;

/// Mutations carry the version captured by the owning runtime, never a worker's wall clock.
#[derive(Clone, Serialize, Deserialize)]
pub enum HistoryMutation {
    Create {
        conversation: AiConversation,
        revision: u64,
    },
    CreateAgentHistory {
        conversation_id: String,
        run_id: crate::agent::AgentRunId,
        revision: u64,
    },
    Metadata {
        conversation: AiConversation,
        revision: u64,
    },
    Rename {
        conversation_id: String,
        title: String,
        updated_at: i64,
        revision: u64,
    },
    PutMessage {
        conversation_id: String,
        branch_id: String,
        message: AiChatMessage,
        revision: u64,
    },
    StreamText {
        conversation_id: String,
        branch_id: String,
        message_id: String,
        delta: HistoryStreamDelta,
    },
    DeleteMessage {
        conversation_id: String,
        branch_id: String,
        message_id: String,
        revision: u64,
    },
    DeleteConversation {
        conversation_id: String,
        revision: u64,
    },
    DeleteBranch {
        conversation_id: String,
        branch_id: String,
        revision: u64,
    },
    Fork {
        conversation_id: String,
        source_branch: String,
        branch_id: String,
        through_sequence: Option<u64>,
        revision: u64,
    },
    ReplaceTail {
        conversation_id: String,
        source_branch: String,
        branch_id: String,
        after_message: Option<String>,
        before_message: Option<String>,
        messages: Vec<AiChatMessage>,
        revision: u64,
    },
    Compact {
        conversation_id: String,
        source_branch: String,
        branch_id: String,
        through_message: String,
        anchor: AiChatMessage,
        revision: u64,
    },
    SelectBranch {
        conversation_id: String,
        branch_id: String,
        revision: u64,
    },
    PutEvent {
        conversation_id: String,
        family: String,
        id: String,
        value: Value,
        revision: u64,
    },
}

impl HistoryMutation {
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn collection_owner(&self) -> Option<&str> {
        match self {
            Self::PutMessage {
                conversation_id, ..
            }
            | Self::DeleteMessage {
                conversation_id, ..
            }
            | Self::DeleteBranch {
                conversation_id, ..
            }
            | Self::ReplaceTail {
                conversation_id, ..
            }
            | Self::Compact {
                conversation_id, ..
            } => Some(conversation_id),
            _ => None,
        }
    }
    pub fn sanitize(&mut self) {
        match self {
            Self::Create { conversation, .. } | Self::Metadata { conversation, .. } => {
                conversation.title = crate::sanitize_for_persistence(&conversation.title);
                if let Some(metadata) = &mut conversation.session_metadata {
                    *metadata = crate::sanitize_tool_protocol_json_for_persistence(metadata);
                }
            }
            Self::Rename { title, .. } => *title = crate::sanitize_for_persistence(title),
            Self::PutMessage { message, .. }
            | Self::Compact {
                anchor: message, ..
            } => crate::context_sanitizer::sanitize_chat_message_for_persistence(message),
            Self::ReplaceTail { messages, .. } => {
                for message in messages {
                    crate::context_sanitizer::sanitize_chat_message_for_persistence(message);
                }
            }
            Self::PutEvent { family, value, .. } => {
                *value = if family == "diagnostic" {
                    crate::sanitize_json_for_ai(value)
                } else {
                    crate::sanitize_tool_protocol_json_for_persistence(value)
                }
            }
            // The delta factory diffs complete sanitized fields, including cross-token replacements.
            Self::StreamText { .. } => {}
            _ => {}
        }
    }
}

#[derive(Clone)]
pub struct HistoryStreamSnapshot {
    message_id: String,
    revision: u64,
    content: Arc<Zeroizing<String>>,
    thinking: Option<Arc<Zeroizing<String>>>,
    parts: usize,
    last_text: Option<Arc<Zeroizing<String>>>,
    turn_exists: bool,
    fields: fields::MessageFields,
    tool_links: Arc<std::collections::BTreeMap<String, ToolParts>>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct HistoryStreamDelta {
    pub(crate) base_revision: u64,
    pub(crate) revision: u64,
    pub(crate) fields: Vec<FieldEdit>,
    pub(crate) tool_parts: std::collections::BTreeMap<String, Option<ToolParts>>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct FieldEdit {
    pub(crate) path: Vec<String>,
    pub(crate) change: Change,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) enum Change {
    Text {
        keep: u64,
        append: Zeroizing<String>,
    },
    Append(Vec<Value>),
    Set(Value),
    Remove,
}

impl HistoryStreamSnapshot {
    pub fn capture(message: &AiChatMessage, revision: u64) -> Self {
        let mut snapshot = Self::capture_text(message, revision);
        snapshot.fields = fields::MessageFields::capture(message);
        snapshot.tool_links = Arc::new(tool_part_links(message));
        snapshot
    }

    fn capture_text(message: &AiChatMessage, revision: u64) -> Self {
        let content = Arc::new(Zeroizing::new(crate::sanitize_for_persistence(
            &message.content,
        )));
        let thinking = message
            .thinking_content
            .as_ref()
            .map(|text| Arc::new(Zeroizing::new(crate::sanitize_for_persistence(text))));
        let parts = message
            .turn
            .as_ref()
            .and_then(|turn| turn.get("parts"))
            .and_then(Value::as_array);
        let last_text = parts
            .and_then(|parts| parts.last())
            .and_then(|part| part.get("text"))
            .and_then(Value::as_str)
            .map(|text| {
                if text == message.content {
                    content.clone()
                } else if message.thinking_content.as_deref() == Some(text) {
                    thinking.as_ref().unwrap().clone()
                } else {
                    Arc::new(Zeroizing::new(crate::sanitize_for_persistence(text)))
                }
            });
        Self {
            message_id: message.id.clone(),
            revision,
            content,
            thinking,
            parts: parts.map_or(0, Vec::len),
            last_text,
            turn_exists: parts.is_some(),
            fields: fields::MessageFields::default(),
            tool_links: Arc::default(),
        }
    }

    /// Diff sanitized fields, never individual raw tokens: a new suffix may redact an earlier prefix.
    pub fn delta(
        &self,
        message: &AiChatMessage,
        revision: u64,
    ) -> Option<(HistoryStreamDelta, Self)> {
        self.delta_inner(message, revision, false)
    }

    pub fn message_delta(
        &self,
        message: &AiChatMessage,
        revision: u64,
    ) -> Option<(HistoryStreamDelta, Self)> {
        self.delta_inner(message, revision, true)
    }

    fn delta_inner(
        &self,
        message: &AiChatMessage,
        revision: u64,
        structural: bool,
    ) -> Option<(HistoryStreamDelta, Self)> {
        if self.message_id != message.id || message.role != AiChatRole::Assistant {
            return None;
        }
        let mut current = Self::capture_text(message, revision);
        if current.parts < self.parts || (self.thinking.is_some() && current.thinking.is_none()) {
            return None;
        }
        let mut fields = Vec::new();
        add_text(
            &mut fields,
            vec!["content".into()],
            &self.content,
            &current.content,
        );
        if let Some(thinking) = &current.thinking {
            add_text(
                &mut fields,
                vec!["thinking_content".into()],
                self.thinking.as_ref().map_or("", |text| text.as_str()),
                thinking,
            );
        }
        if !self.turn_exists && current.turn_exists {
            let turn = message.turn.as_ref()?.as_object()?;
            let mut header: serde_json::Map<_, _> = turn
                .iter()
                .filter(|(key, _)| key.as_str() != "parts")
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect();
            header.insert("parts".into(), Value::Array(Vec::new()));
            fields.push(FieldEdit {
                path: vec!["turn".into()],
                change: Change::Set(crate::sanitize_tool_protocol_json_for_persistence(
                    &Value::Object(header),
                )),
            });
        }
        if let Some(parts) = message
            .turn
            .as_ref()
            .and_then(|turn| turn.get("parts"))
            .and_then(Value::as_array)
        {
            if !structural
                && parts[self.parts..].iter().any(|part| {
                    !matches!(
                        part.get("type").and_then(Value::as_str),
                        Some("text" | "thinking")
                    )
                })
            {
                return None;
            }
            if let Some(previous) = &self.last_text {
                let text = parts
                    .get(self.parts.checked_sub(1)?)?
                    .get("text")?
                    .as_str()?;
                let safe = Zeroizing::new(crate::sanitize_for_persistence(text));
                add_text(
                    &mut fields,
                    vec![
                        "turn".into(),
                        "parts".into(),
                        (self.parts - 1).to_string(),
                        "text".into(),
                    ],
                    previous,
                    &safe,
                );
            }
            if parts.len() > self.parts {
                fields.push(FieldEdit {
                    path: vec!["turn".into(), "parts".into()],
                    change: Change::Append(
                        parts[self.parts..]
                            .iter()
                            .map(crate::sanitize_tool_protocol_json_for_persistence)
                            .collect(),
                    ),
                });
            }
        }
        let tool_parts = if structural {
            current.fields = fields::MessageFields::capture_next(message, Some(&self.fields));
            self.fields
                .edits(&current.fields, message, self.parts, &mut fields)?;
            let links = tool_part_links(message);
            let mut changed = std::collections::BTreeMap::new();
            for (id, link) in &links {
                if self.tool_links.get(id) != Some(link) {
                    changed.insert(id.clone(), Some(link.clone()));
                }
            }
            for id in self.tool_links.keys().filter(|id| !links.contains_key(*id)) {
                changed.insert(id.clone(), None);
            }
            current.tool_links = Arc::new(links);
            changed
        } else {
            current.fields = self.fields.advance_text(message, self.parts);
            current.tool_links = self.tool_links.clone();
            std::collections::BTreeMap::new()
        };
        Some((
            HistoryStreamDelta {
                base_revision: self.revision,
                revision,
                fields,
                tool_parts,
            },
            current,
        ))
    }
}

fn add_text(fields: &mut Vec<FieldEdit>, path: Vec<String>, previous: &str, current: &str) {
    if previous == current {
        return;
    }
    let mut keep = previous
        .bytes()
        .zip(current.bytes())
        .take_while(|(left, right)| left == right)
        .count();
    while !previous.is_char_boundary(keep) || !current.is_char_boundary(keep) {
        keep -= 1;
    }
    fields.push(FieldEdit {
        path,
        change: Change::Text {
            keep: keep as u64,
            append: Zeroizing::new(current[keep..].to_owned()),
        },
    });
}

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ToolParts {
    pub(crate) call: Option<u64>,
    pub(crate) result: Option<u64>,
    pub(crate) projection: Option<u64>,
}

pub(crate) fn tool_part_links(
    message: &AiChatMessage,
) -> std::collections::BTreeMap<String, ToolParts> {
    let mut links = std::collections::BTreeMap::<String, ToolParts>::new();
    for (index, call) in message.tool_calls.iter().enumerate() {
        if let Some(id) = call.get("id").and_then(Value::as_str) {
            links.entry(id.into()).or_default().projection = Some(index as u64);
        }
    }
    if let Some(parts) = message
        .turn
        .as_ref()
        .and_then(|turn| turn.get("parts"))
        .and_then(Value::as_array)
    {
        for (index, part) in parts.iter().enumerate() {
            match part.get("type").and_then(Value::as_str) {
                Some("tool_call") => {
                    if let Some(id) = part.get("id").and_then(Value::as_str) {
                        links.entry(id.into()).or_default().call = Some(index as u64);
                    }
                }
                Some("tool_result") => {
                    if let Some(id) = part.get("toolCallId").and_then(Value::as_str) {
                        links.entry(id.into()).or_default().result = Some(index as u64);
                    }
                }
                _ => {}
            }
        }
    }
    links
}

pub(crate) fn identity(value: &Value) -> anyhow::Result<String> {
    struct HashWriter(Sha256);
    impl std::io::Write for HashWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = HashWriter(Sha256::new());
    CanonicalValue(value).serialize(&mut rmp_serde::Serializer::new(&mut writer))?;
    Ok(format!("{:x}", writer.0.finalize()))
}

struct CanonicalValue<'a>(&'a Value);
impl Serialize for CanonicalValue<'_> {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        match self.0 {
            Value::Object(fields) => {
                let mut fields: Vec<_> = fields.iter().collect();
                fields.sort_unstable_by_key(|(key, _)| *key);
                let mut map = serializer.serialize_map(Some(fields.len()))?;
                for (key, value) in fields {
                    map.serialize_entry(key, &CanonicalValue(value))?;
                }
                map.end()
            }
            Value::Array(values) => {
                let mut sequence = serializer.serialize_seq(Some(values.len()))?;
                for value in values {
                    sequence.serialize_element(&CanonicalValue(value))?;
                }
                sequence.end()
            }
            value => value.serialize(serializer),
        }
    }
}
