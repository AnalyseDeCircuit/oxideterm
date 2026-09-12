use super::{records::*, *};
use zeroize::Zeroizing;
mod fields;

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
    tool_links: Arc<std::collections::BTreeMap<String, super::windows::ToolParts>>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct HistoryStreamDelta {
    base_revision: u64,
    revision: u64,
    fields: Vec<FieldEdit>,
    tool_parts: std::collections::BTreeMap<String, Option<super::windows::ToolParts>>,
}

#[derive(Clone, Serialize, Deserialize)]
struct FieldEdit {
    path: Vec<String>,
    change: Change,
}

#[derive(Clone, Serialize, Deserialize)]
enum Change {
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
        snapshot.tool_links = Arc::new(super::windows::tool_part_links(message));
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
            let links = super::windows::tool_part_links(message);
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

pub(super) fn apply(
    tx: &redb::WriteTransaction,
    conversation: &str,
    branch: &str,
    message_id: &str,
    delta: HistoryStreamDelta,
) -> Result<()> {
    let Some(mut head) = tx
        .open_table(HEADS)?
        .get(conversation)?
        .map(|row| rmp_serde::from_slice::<ConversationHead>(row.value()))
        .transpose()?
    else {
        return Ok(());
    };
    if tx
        .open_table(MESSAGE_DELETED)?
        .get((conversation, branch, message_id))?
        .is_some()
    {
        return Ok(());
    }
    let Some(sequence) = tx
        .open_table(IDS)?
        .get((conversation, branch, message_id))?
        .map(|row| row.value())
    else {
        return Ok(());
    };
    let key = tx
        .open_table(ORDER)?
        .get((conversation, branch, sequence))?
        .ok_or_else(|| anyhow!("History index is missing"))?
        .value()
        .to_owned();
    let mut description: MessageDescriptor = rmp_serde::from_slice(
        tx.open_table(DESCRIPTORS)?
            .get((conversation, key.as_str()))?
            .ok_or_else(|| anyhow!("History description is missing"))?
            .value(),
    )?;
    if description.revision >= delta.revision {
        return Ok(());
    }
    let branch: Branch = rmp_serde::from_slice(
        tx.open_table(BRANCHES)?
            .get((conversation, branch))?
            .ok_or_else(|| anyhow!("History branch is missing"))?
            .value(),
    )?;
    if branch.sealed || description.revision != delta.base_revision {
        return Err(anyhow!("History text base changed"));
    }
    let mut value: StoredValue = rmp_serde::from_slice(
        tx.open_table(MESSAGES)?
            .get((conversation, key.as_str()))?
            .ok_or_else(|| anyhow!("History message is missing"))?
            .value(),
    )?;
    for field in delta.fields {
        value = edit(tx, conversation, value, &field.path, field.change)?;
    }
    if !delta.tool_parts.is_empty() {
        let mut table = tx.open_table(TOOL_PARTS)?;
        for (id, link) in delta.tool_parts {
            if let Some(link) = link {
                table.insert(
                    (conversation, key.as_str(), id.as_str()),
                    rmp_serde::to_vec(&link)?.as_slice(),
                )?;
            } else {
                table.remove((conversation, key.as_str(), id.as_str()))?;
            }
        }
    }
    description.preview = preview(tx, conversation, &value)?;
    description.revision = delta.revision;
    tx.open_table(MESSAGES)?.insert(
        (conversation, key.as_str()),
        rmp_serde::to_vec(&value)?.as_slice(),
    )?;
    tx.open_table(DESCRIPTORS)?.insert(
        (conversation, key.as_str()),
        rmp_serde::to_vec_named(&description)?.as_slice(),
    )?;
    head.revision = head.revision.max(delta.revision);
    super::mutations::write_head(tx, head)
}

fn edit(
    tx: &redb::WriteTransaction,
    conversation: &str,
    value: StoredValue,
    path: &[String],
    change: Change,
) -> Result<StoredValue> {
    let Some((key, rest)) = path.split_first() else {
        return apply_field(tx, conversation, value, change);
    };
    Ok(match value {
        StoredValue::Object(mut fields) => {
            let previous = fields
                .remove(key)
                .unwrap_or(StoredValue::Scalar(Value::Null));
            if rest.is_empty() && matches!(change, Change::Remove) {
                super::content::release_value(tx, conversation, previous)?;
            } else {
                fields.insert(
                    key.clone(),
                    edit_object_field(tx, conversation, key, previous, rest, change)?,
                );
            }
            StoredValue::Object(fields)
        }
        StoredValue::ObjectRef { id } => {
            let previous = tx
                .open_table(OBJECT_FIELDS)?
                .get((conversation, id.as_str(), key.as_str()))?
                .map(|row| rmp_serde::from_slice(row.value()))
                .transpose()?
                .unwrap_or(StoredValue::Scalar(Value::Null));
            if rest.is_empty() && matches!(change, Change::Remove) {
                super::content::release_value(tx, conversation, previous)?;
                tx.open_table(OBJECT_FIELDS)?
                    .remove((conversation, id.as_str(), key.as_str()))?;
            } else {
                let next = edit_object_field(tx, conversation, key, previous, rest, change)?;
                tx.open_table(OBJECT_FIELDS)?.insert(
                    (conversation, id.as_str(), key.as_str()),
                    rmp_serde::to_vec(&next)?.as_slice(),
                )?;
            }
            StoredValue::ObjectRef { id }
        }
        StoredValue::Array { id, length } => {
            let index: u64 = key.parse()?;
            if index >= length {
                return Err(anyhow!("History array offset is invalid"));
            }
            let previous = rmp_serde::from_slice(
                tx.open_table(ARRAY_ITEMS)?
                    .get((conversation, id.as_str(), index))?
                    .ok_or_else(|| anyhow!("History part is missing"))?
                    .value(),
            )?;
            let next = edit(tx, conversation, previous, rest, change)?;
            tx.open_table(ARRAY_ITEMS)?.insert(
                (conversation, id.as_str(), index),
                rmp_serde::to_vec(&next)?.as_slice(),
            )?;
            StoredValue::Array { id, length }
        }
        _ => return Err(anyhow!("History field is unavailable")),
    })
}

fn edit_object_field(
    tx: &redb::WriteTransaction,
    conversation: &str,
    key: &str,
    previous: StoredValue,
    rest: &[String],
    change: Change,
) -> Result<StoredValue> {
    match (rest.is_empty(), change) {
        (true, Change::Set(value)) => {
            super::content::update_field(tx, conversation, key, Some(previous), value)
        }
        (_, change) => edit(tx, conversation, previous, rest, change),
    }
}

fn apply_field(
    tx: &redb::WriteTransaction,
    conversation: &str,
    value: StoredValue,
    change: Change,
) -> Result<StoredValue> {
    match change {
        Change::Text { keep, append } => match value {
            StoredValue::Text { id, bytes } => {
                super::text::replace_tail(tx, conversation, id, bytes, keep, &append)
            }
            value => {
                let mut text = match value {
                    StoredValue::Scalar(Value::String(text)) => Zeroizing::new(text),
                    StoredValue::Scalar(Value::Null) => Zeroizing::new(String::new()),
                    _ => return Err(anyhow!("History text is invalid")),
                };
                let keep = usize::try_from(keep)?;
                if keep > text.len() || !text.is_char_boundary(keep) {
                    return Err(anyhow!("History text boundary is invalid"));
                }
                text.truncate(keep);
                text.push_str(&append);
                super::content::store_value(
                    tx,
                    conversation,
                    Value::String(std::mem::take(&mut *text)),
                )
            }
        },
        Change::Append(values) => {
            let (id, mut length) = match value {
                StoredValue::Array { id, length } => (id, length),
                StoredValue::Scalar(Value::Array(values)) if values.is_empty() => {
                    (uuid::Uuid::new_v4().to_string(), 0)
                }
                _ => return Err(anyhow!("History array is invalid")),
            };
            for value in values {
                let value = super::content::store_value(tx, conversation, value)?;
                tx.open_table(ARRAY_ITEMS)?.insert(
                    (conversation, id.as_str(), length),
                    rmp_serde::to_vec(&value)?.as_slice(),
                )?;
                length += 1;
            }
            Ok(StoredValue::Array { id, length })
        }
        Change::Remove => Err(anyhow!("History removal requires an object field")),
        Change::Set(next) => super::content::update_value(tx, conversation, value, next),
    }
}

fn preview(tx: &redb::WriteTransaction, conversation: &str, value: &StoredValue) -> Result<String> {
    let content = match value {
        StoredValue::Object(fields) => fields.get("content").cloned(),
        StoredValue::ObjectRef { id } => tx
            .open_table(OBJECT_FIELDS)?
            .get((conversation, id.as_str(), "content"))?
            .map(|row| rmp_serde::from_slice(row.value()))
            .transpose()?,
        _ => None,
    }
    .ok_or_else(|| anyhow!("History text is missing"))?;
    match content {
        StoredValue::Scalar(Value::String(text)) => Ok(text.chars().take(256).collect()),
        StoredValue::Text { id, bytes: 0 } => {
            let _ = id;
            Ok(String::new())
        }
        StoredValue::Text { id, .. } => super::text::preview(tx, conversation, &id),
        _ => Err(anyhow!("History text is invalid")),
    }
}
