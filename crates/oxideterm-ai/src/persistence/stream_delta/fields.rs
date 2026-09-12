use super::*;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Clone)]
struct ProtocolField {
    fingerprint: String,
    value: Arc<Value>,
}

#[derive(Clone, Default)]
struct ProtocolFields {
    scalar: Option<Arc<Value>>,
    fields: BTreeMap<String, ProtocolField>,
}
impl ProtocolFields {
    fn capture(value: &Value, skip: Option<&str>, card: bool, previous: Option<&Self>) -> Self {
        let Some(object) = value.as_object() else {
            return Self {
                scalar: Some(Arc::new(
                    crate::sanitize_tool_protocol_json_for_persistence(value),
                )),
                fields: BTreeMap::new(),
            };
        };
        let tool_name = object
            .get("name")
            .or_else(|| object.get("toolName"))
            .and_then(Value::as_str);
        let mut fields = BTreeMap::new();
        for (key, value) in object {
            if Some(key.as_str()) == skip {
                continue;
            }
            // Raw fingerprints only live with the running message; unchanged safe payloads are shared.
            let fingerprint = format!(
                "{:x}:{}",
                Sha256::digest(tool_name.unwrap_or_default().as_bytes()),
                super::super::tool_payloads::identity(value)
                    .expect("JSON values serialize to the infallible hash writer")
            );
            if let Some(field) = previous
                .and_then(|previous| previous.fields.get(key))
                .filter(|field| field.fingerprint == fingerprint)
            {
                fields.insert(key.clone(), field.clone());
                continue;
            }
            if let Some(value) =
                crate::context_sanitizer::sanitize_tool_protocol_field_for_persistence(
                    key, value, tool_name,
                )
            {
                fields.insert(
                    key.clone(),
                    ProtocolField {
                        fingerprint,
                        value: Arc::new(value),
                    },
                );
            }
        }
        if card {
            for (key, value) in [("historical", true), ("actionable", false)] {
                fields.insert(
                    key.into(),
                    ProtocolField {
                        fingerprint: String::new(),
                        value: Arc::new(Value::Bool(value)),
                    },
                );
            }
        }
        Self {
            scalar: None,
            fields,
        }
    }
    fn value(&self) -> Value {
        self.scalar
            .as_ref()
            .map(|value| (**value).clone())
            .unwrap_or_else(|| {
                Value::Object(
                    self.fields
                        .iter()
                        .map(|(key, field)| (key.clone(), (*field.value).clone()))
                        .collect(),
                )
            })
    }
    fn edits(&self, next: &Self, path: &mut Vec<String>, edits: &mut Vec<FieldEdit>) {
        if self.scalar.is_some() || next.scalar.is_some() {
            diff(&self.value(), &next.value(), path, edits);
            return;
        }
        let changed = |key: &str| match (self.fields.get(key), next.fields.get(key)) {
            (Some(before), Some(after)) => {
                !Arc::ptr_eq(&before.value, &after.value) && before.value != after.value
            }
            (None, None) => false,
            _ => true,
        };
        if next.fields.contains_key("envelope")
            && next.fields.contains_key("output")
            && (changed("envelope") || changed("output"))
        {
            edits.push(FieldEdit {
                path: path.clone(),
                change: Change::Set(next.value()),
            });
            return;
        }
        for (key, field) in &next.fields {
            if !changed(key) {
                continue;
            }
            path.push(key.clone());
            if let Some(previous) = self.fields.get(key) {
                diff(&previous.value, &field.value, path, edits);
            } else {
                edits.push(FieldEdit {
                    path: path.clone(),
                    change: Change::Set((*field.value).clone()),
                });
            }
            path.pop();
        }
        for key in self
            .fields
            .keys()
            .filter(|key| !next.fields.contains_key(*key))
        {
            path.push(key.clone());
            edits.push(FieldEdit {
                path: path.clone(),
                change: Change::Remove,
            });
            path.pop();
        }
    }
}

#[derive(Clone, Default)]
pub(super) struct MessageFields {
    header: Arc<Value>,
    turn: Arc<ProtocolFields>,
    tools: Vec<Arc<ProtocolFields>>,
    parts: Vec<Arc<ProtocolFields>>,
    text_hashes: Vec<Option<[u8; 32]>>,
}

impl MessageFields {
    pub fn capture(message: &AiChatMessage) -> Self {
        Self::capture_next(message, None)
    }
    pub fn capture_next(message: &AiChatMessage, previous: Option<&Self>) -> Self {
        let header = serde_json::json!({
            "id":message.id, "role":message.role, "timestamp_ms":message.timestamp_ms,
            "model":message.model, "context":message.context, "is_streaming":message.is_streaming,
            "tool_call_id":message.tool_call_id, "metadata":message.metadata, "branches":message.branches,
            "transcript_ref":message.transcript_ref, "summary_ref":message.summary_ref, "suggestions":message.suggestions,
        });
        let turn = ProtocolFields::capture(
            message.turn.as_ref().unwrap_or(&Value::Null),
            Some("parts"),
            false,
            previous.map(|previous| &*previous.turn),
        );
        let tools = message
            .tool_calls
            .iter()
            .enumerate()
            .map(|(index, call)| {
                Arc::new(ProtocolFields::capture(
                    call,
                    None,
                    true,
                    previous
                        .and_then(|previous| previous.tools.get(index))
                        .map(Arc::as_ref),
                ))
            })
            .collect();
        let mut result = Self {
            header: Arc::new(crate::sanitize_tool_protocol_json_for_persistence(&header)),
            turn: Arc::new(turn),
            tools,
            parts: Vec::new(),
            text_hashes: Vec::new(),
        };
        if let Some(parts) = message
            .turn
            .as_ref()
            .and_then(|turn| turn.get("parts"))
            .and_then(Value::as_array)
        {
            for (index, part) in parts.iter().enumerate() {
                result.push_part(
                    part,
                    previous
                        .and_then(|previous| previous.parts.get(index))
                        .map(Arc::as_ref),
                );
            }
        }
        result
    }

    fn push_part(&mut self, part: &Value, previous: Option<&ProtocolFields>) {
        let text = matches!(
            part.get("type").and_then(Value::as_str),
            Some("text" | "thinking")
        );
        self.parts.push(Arc::new(ProtocolFields::capture(
            part,
            text.then_some("text"),
            false,
            previous,
        )));
        self.text_hashes.push(if text {
            part.get("text").and_then(Value::as_str).map(text_hash)
        } else {
            None
        });
    }

    pub fn advance_text(&self, message: &AiChatMessage, previous_parts: usize) -> Self {
        let mut next = self.clone();
        if next
            .turn
            .scalar
            .as_ref()
            .is_some_and(|value| value.is_null())
        {
            next.turn = Arc::new(ProtocolFields::capture(
                message.turn.as_ref().unwrap_or(&Value::Null),
                Some("parts"),
                false,
                None,
            ));
        }
        if let Some(parts) = message
            .turn
            .as_ref()
            .and_then(|turn| turn.get("parts"))
            .and_then(Value::as_array)
        {
            if previous_parts > 0 {
                next.text_hashes[previous_parts - 1] = parts[previous_parts - 1]
                    .get("text")
                    .and_then(Value::as_str)
                    .map(text_hash);
            }
            for part in &parts[previous_parts..] {
                next.push_part(part, None);
            }
        }
        next
    }

    pub fn edits(
        &self,
        next: &Self,
        message: &AiChatMessage,
        previous_parts: usize,
        edits: &mut Vec<FieldEdit>,
    ) -> Option<()> {
        // Archive ownership and message identity still require the structural mutation path.
        for key in [
            "id",
            "role",
            "timestamp_ms",
            "metadata",
            "branches",
            "summary_ref",
        ] {
            if self.header.get(key) != next.header.get(key) {
                return None;
            }
        }
        if next.tools.len() < self.tools.len() {
            return None;
        }
        diff(&self.header, &next.header, &mut Vec::new(), edits);
        for (index, (previous, next)) in self.tools.iter().zip(&next.tools).enumerate() {
            previous.edits(
                next,
                &mut vec!["tool_calls".into(), index.to_string()],
                edits,
            );
        }
        if next.tools.len() > self.tools.len() {
            edits.push(FieldEdit {
                path: vec!["tool_calls".into()],
                change: Change::Append(
                    next.tools[self.tools.len()..]
                        .iter()
                        .map(|tool| tool.value())
                        .collect(),
                ),
            });
        }
        if self.turn.scalar.is_none() && next.turn.scalar.is_none() {
            self.turn.edits(&next.turn, &mut vec!["turn".into()], edits);
        } else if message
            .turn
            .as_ref()
            .and_then(|turn| turn.get("parts"))
            .is_none()
        {
            self.turn.edits(&next.turn, &mut vec!["turn".into()], edits);
        }
        for index in 0..previous_parts {
            self.parts[index].edits(
                &next.parts[index],
                &mut vec!["turn".into(), "parts".into(), index.to_string()],
                edits,
            );
            if index + 1 < previous_parts && self.text_hashes[index] != next.text_hashes[index] {
                let text = message
                    .turn
                    .as_ref()?
                    .get("parts")?
                    .get(index)?
                    .get("text")?
                    .as_str()?;
                edits.push(FieldEdit {
                    path: vec![
                        "turn".into(),
                        "parts".into(),
                        index.to_string(),
                        "text".into(),
                    ],
                    change: Change::Set(Value::String(crate::sanitize_for_persistence(text))),
                });
            }
        }
        Some(())
    }
}

fn text_hash(text: &str) -> [u8; 32] {
    Sha256::digest(text.as_bytes()).into()
}

fn diff(previous: &Value, next: &Value, path: &mut Vec<String>, edits: &mut Vec<FieldEdit>) {
    if previous == next {
        return;
    }
    // Canonical payloads are immutable shared values; replace their reference as one field.
    let shared = path.last().is_some_and(|key| {
        matches!(
            key.as_str(),
            "result" | "output" | "envelope" | "arguments" | "argumentsText"
        )
    });
    if !shared {
        match (previous, next) {
            (Value::Object(before), Value::Object(after)) => {
                if after.contains_key("envelope")
                    && after.contains_key("output")
                    && (before.get("envelope") != after.get("envelope")
                        || before.get("output") != after.get("output"))
                {
                    edits.push(FieldEdit {
                        path: path.clone(),
                        change: Change::Set(next.clone()),
                    });
                    return;
                }
                for (key, value) in after {
                    path.push(key.clone());
                    if let Some(previous) = before.get(key) {
                        diff(previous, value, path, edits);
                    } else {
                        edits.push(FieldEdit {
                            path: path.clone(),
                            change: Change::Set(value.clone()),
                        });
                    }
                    path.pop();
                }
                for key in before.keys().filter(|key| !after.contains_key(*key)) {
                    path.push(key.clone());
                    edits.push(FieldEdit {
                        path: path.clone(),
                        change: Change::Remove,
                    });
                    path.pop();
                }
                return;
            }
            (Value::Array(before), Value::Array(after)) if after.len() >= before.len() => {
                for (index, (previous, next)) in before.iter().zip(after).enumerate() {
                    path.push(index.to_string());
                    diff(previous, next, path, edits);
                    path.pop();
                }
                if after.len() > before.len() {
                    edits.push(FieldEdit {
                        path: path.clone(),
                        change: Change::Append(after[before.len()..].to_vec()),
                    });
                }
                return;
            }
            _ => {}
        }
    }
    edits.push(FieldEdit {
        path: path.clone(),
        change: Change::Set(next.clone()),
    });
}
