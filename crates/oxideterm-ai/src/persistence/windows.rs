use super::{records::*, *};

const WINDOW_BYTES: usize = CONTENT_CHUNK_BYTES;
const WINDOW_ITEMS: usize = HISTORY_PAGE_SIZE;

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ToolParts {
    call: Option<u64>,
    result: Option<u64>,
    projection: Option<u64>,
}

pub(super) fn index_tool_parts(
    tx: &redb::WriteTransaction,
    conversation: &str,
    storage_id: &str,
    message: &AiChatMessage,
) -> Result<()> {
    write_tool_parts(tx, conversation, storage_id, tool_part_links(message))
}

pub(super) fn tool_part_links(
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

pub(super) fn write_tool_parts(
    tx: &redb::WriteTransaction,
    conversation: &str,
    storage_id: &str,
    links: std::collections::BTreeMap<String, ToolParts>,
) -> Result<()> {
    let mut table = tx.open_table(TOOL_PARTS)?;
    let removed = table
        .range((conversation, storage_id, "")..=(conversation, storage_id, "\u{10ffff}"))?
        .filter_map(|row| match row {
            Ok((key, _)) if !links.contains_key(key.value().2) => {
                Some(Ok(key.value().2.to_owned()))
            }
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for key in removed {
        table.remove((conversation, storage_id, key.as_str()))?;
    }
    for (id, links) in links {
        let bytes = rmp_serde::to_vec(&links)?;
        let changed = table
            .get((conversation, storage_id, id.as_str()))?
            .is_none_or(|row| row.value() != bytes.as_slice());
        if changed {
            table.insert((conversation, storage_id, id.as_str()), bytes.as_slice())?;
        }
    }
    Ok(())
}

pub(super) fn delete_tool_parts(
    tx: &redb::WriteTransaction,
    conversation: &str,
    storage_id: &str,
) -> Result<()> {
    let mut table = tx.open_table(TOOL_PARTS)?;
    let keys = table
        .range((conversation, storage_id, "")..=(conversation, storage_id, "\u{10ffff}"))?
        .map(|row| row.map(|(key, _)| key.value().2.to_owned()))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for key in keys {
        table.remove((conversation, storage_id, key.as_str()))?;
    }
    Ok(())
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
    pub section: u64,
    pub sections: u64,
    pub more: Vec<HistoryContentCursor>,
}

impl ConversationStore {
    pub fn message_content(
        &self,
        conversation: &str,
        storage_id: &str,
        revision: u64,
    ) -> Result<String> {
        let guard = self.db.read();
        let tx = guard
            .as_ref()
            .ok_or_else(|| anyhow!("History database is unavailable"))?
            .begin_read()?;
        let description: MessageDescriptor = rmp_serde::from_slice(
            tx.open_table(DESCRIPTORS)?
                .get((conversation, storage_id))?
                .ok_or_else(|| anyhow!("History message is missing"))?
                .value(),
        )?;
        if description.revision != revision {
            return Err(anyhow!("History content cursor expired"));
        }
        let root: StoredValue = rmp_serde::from_slice(
            tx.open_table(MESSAGES)?
                .get((conversation, storage_id))?
                .ok_or_else(|| anyhow!("History message is missing"))?
                .value(),
        )?;
        let value = resolve(&tx, conversation, root, &["content".into()])?;
        match super::content::load_value(&tx, conversation, value, &self.cache)? {
            Value::String(text) => Ok(text),
            _ => Err(anyhow!("History text is invalid")),
        }
    }

    pub fn message_view(
        &self,
        conversation: &str,
        storage_id: &str,
        revision: u64,
        section: Option<u64>,
    ) -> Result<Arc<HistoryMessageView>> {
        let guard = self.db.read();
        let tx = guard
            .as_ref()
            .ok_or_else(|| anyhow!("History database is unavailable"))?
            .begin_read()?;
        let descriptions = tx.open_table(DESCRIPTORS)?;
        let description: MessageDescriptor = rmp_serde::from_slice(
            descriptions
                .get((conversation, storage_id))?
                .ok_or_else(|| anyhow!("History message is missing"))?
                .value(),
        )?;
        if revision != description.revision {
            return Err(anyhow!("History content cursor expired"));
        }
        let table = tx.open_table(MESSAGES)?;
        let root: StoredValue = rmp_serde::from_slice(
            table
                .get((conversation, storage_id))?
                .ok_or_else(|| anyhow!("History message is missing"))?
                .value(),
        )?;
        let mut fields = super::content::object_fields(&tx, conversation, root.clone())?;
        let parts = fields
            .get("turn")
            .cloned()
            .filter(|value| !matches!(value, StoredValue::Scalar(Value::Null)))
            .map(|turn| super::content::object_fields(&tx, conversation, turn))
            .transpose()?
            .and_then(|mut fields| fields.remove("parts"))
            .map(array_length)
            .transpose()?
            .unwrap_or(0);
        let tools = fields
            .get("tool_calls")
            .cloned()
            .map(array_length)
            .transpose()?
            .unwrap_or(0);
        let thinking = fields
            .get("thinking_content")
            .is_some_and(|value| !matches!(value, StoredValue::Scalar(Value::Null)));
        let sections = if parts > 0 {
            parts
        } else {
            1 + u64::from(thinking) + tools
        };
        let section = section.unwrap_or_else(|| if parts > 0 { parts - 1 } else { 0 });
        if section >= sections {
            return Err(anyhow!("History section is unavailable"));
        }
        let cache_key = format!("message:{storage_id}:{revision}:{section}");
        if let Some(view) = self.cache.lock().get_message(conversation, &cache_key) {
            return Ok(view);
        }
        for field in [
            "content",
            "context",
            "thinking_content",
            "tool_calls",
            "turn",
            "suggestions",
        ] {
            fields.remove(field);
        }
        fields.insert(
            "content".into(),
            StoredValue::Scalar(Value::String(String::new())),
        );
        let mut message: AiChatMessage = serde_json::from_value(super::content::load_value(
            &tx,
            conversation,
            StoredValue::Object(fields),
            &self.cache,
        )?)?;
        let cursor = HistoryContentCursor {
            event: None,
            conversation_id: conversation.into(),
            storage_id: storage_id.into(),
            revision,
            path: Vec::new(),
            offset: 0,
            byte_offset: 0,
            after_key: None,
        };
        let mut more = Vec::new();
        let mut read = |path: Vec<String>| -> Result<Value> {
            let mut cursor = cursor.clone();
            cursor.path = path;
            let source = resolve(&tx, conversation, root.clone(), &cursor.path)?;
            let identity = match &source {
                StoredValue::Object(_) | StoredValue::ObjectRef { .. } => Some(
                    super::content::object_fields(&tx, conversation, source.clone())?,
                ),
                _ => None,
            };
            let mut window = Window {
                tx: &tx,
                conversation,
                cache: &self.cache,
                bytes: WINDOW_BYTES,
                items: WINDOW_ITEMS,
                more: Vec::new(),
                visiting: HashSet::new(),
            };
            let mut value = window.read(source, cursor)?;
            // Card identity and execution state are metadata, independent of the output window.
            if let (Some(fields), Some(output)) = (identity, value.as_object_mut()) {
                for key in ["type", "id", "toolCallId", "name", "status"] {
                    if let Some(field) = fields.get(key) {
                        output.insert(
                            key.into(),
                            super::content::load_value(
                                &tx,
                                conversation,
                                field.clone(),
                                &self.cache,
                            )?,
                        );
                    }
                }
            }
            more.extend(window.more);
            Ok(value)
        };
        if parts > 0 {
            let path = vec!["turn".into(), "parts".into(), section.to_string()];
            let fields = super::content::object_fields(
                &tx,
                conversation,
                resolve(&tx, conversation, root.clone(), &path)?,
            )?;
            let mut identity = serde_json::Map::new();
            for key in ["type", "id", "toolCallId"] {
                if let Some(value) = fields.get(key) {
                    identity.insert(
                        key.into(),
                        super::content::load_value(&tx, conversation, value.clone(), &self.cache)?,
                    );
                }
            }
            let part = Value::Object(identity);
            let kind = part.get("type").and_then(Value::as_str).unwrap_or_default();
            if matches!(kind, "tool_call" | "tool_result") {
                let id = part
                    .get("id")
                    .or_else(|| part.get("toolCallId"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("History tool identity is missing"))?;
                let links: ToolParts = tx
                    .open_table(TOOL_PARTS)?
                    .get((conversation, storage_id, id))?
                    .map(|row| rmp_serde::from_slice(row.value()))
                    .transpose()?
                    .ok_or_else(|| anyhow!("History tool index is missing"))?;
                if let Some(index) = links.projection {
                    message
                        .tool_calls
                        .push(read(vec!["tool_calls".into(), index.to_string()])?);
                } else {
                    let mut group = Vec::new();
                    for index in [links.call, links.result].into_iter().flatten() {
                        group.push(read(vec![
                            "turn".into(),
                            "parts".into(),
                            index.to_string(),
                        ])?);
                    }
                    message.turn = Some(serde_json::json!({"parts":group}));
                }
            } else {
                message.turn = Some(serde_json::json!({"parts":[read(path)?]}));
            }
        } else if section == 0 {
            message.content = read(vec!["content".into()])?
                .as_str()
                .ok_or_else(|| anyhow!("History text is invalid"))?
                .to_owned();
        } else if thinking && section == 1 {
            message.thinking_content = Some(
                read(vec!["thinking_content".into()])?
                    .as_str()
                    .ok_or_else(|| anyhow!("History text is invalid"))?
                    .to_owned(),
            );
        } else {
            message.tool_calls.push(read(vec![
                "tool_calls".into(),
                (section - 1 - u64::from(thinking)).to_string(),
            ])?);
        }
        message.is_streaming = false;
        normalize_interrupted_assistant_projection(&mut message);
        let view = Arc::new(HistoryMessageView {
            message,
            section,
            sections,
            more,
        });
        self.cache
            .lock()
            .insert_message(conversation, &cache_key, view.clone());
        Ok(view)
    }

    /// A content window never decodes strings or array entries outside its requested range.
    pub fn content_page(&self, cursor: &HistoryContentCursor) -> Result<HistoryContentPage> {
        let guard = self.db.read();
        let tx = guard
            .as_ref()
            .ok_or_else(|| anyhow!("History database is unavailable"))?
            .begin_read()?;
        let value: StoredValue = if let Some(event) = &cursor.event {
            let revision = tx
                .open_table(EVENT_REVISIONS)?
                .get((
                    cursor.conversation_id.as_str(),
                    event.family.as_str(),
                    event.id.as_str(),
                ))?
                .ok_or_else(|| anyhow!("History event is missing"))?
                .value();
            if revision != cursor.revision {
                return Err(anyhow!("History event cursor expired"));
            }
            rmp_serde::from_slice(
                tx.open_table(EVENTS)?
                    .get((
                        cursor.conversation_id.as_str(),
                        event.family.as_str(),
                        event.sequence,
                    ))?
                    .ok_or_else(|| anyhow!("History event is missing"))?
                    .value(),
            )?
        } else {
            let descriptions = tx.open_table(DESCRIPTORS)?;
            let description: MessageDescriptor = rmp_serde::from_slice(
                descriptions
                    .get((cursor.conversation_id.as_str(), cursor.storage_id.as_str()))?
                    .ok_or_else(|| anyhow!("History message is missing"))?
                    .value(),
            )?;
            if description.revision != cursor.revision {
                return Err(anyhow!("History content cursor expired"));
            }
            let table = tx.open_table(MESSAGES)?;
            let value: StoredValue = rmp_serde::from_slice(
                table
                    .get((cursor.conversation_id.as_str(), cursor.storage_id.as_str()))?
                    .ok_or_else(|| anyhow!("History message is missing"))?
                    .value(),
            )?;
            value
        };
        let value = resolve(&tx, &cursor.conversation_id, value, &cursor.path)?;
        let mut window = Window {
            tx: &tx,
            conversation: &cursor.conversation_id,
            cache: &self.cache,
            bytes: WINDOW_BYTES,
            items: WINDOW_ITEMS,
            more: Vec::new(),
            visiting: HashSet::new(),
        };
        let value = window.read(value, cursor.clone())?;
        Ok(HistoryContentPage {
            value,
            more: window.more,
        })
    }
}

fn array_length(value: StoredValue) -> Result<u64> {
    match value {
        StoredValue::Array { length, .. } => Ok(length),
        StoredValue::Scalar(Value::Array(values)) if values.is_empty() => Ok(0),
        _ => Err(anyhow!("History array is invalid")),
    }
}

fn resolve(
    tx: &redb::ReadTransaction,
    conversation: &str,
    mut value: StoredValue,
    path: &[String],
) -> Result<StoredValue> {
    for component in path {
        value = match super::tool_payloads::resolve(tx, conversation, value)? {
            StoredValue::Object(mut fields) => fields
                .remove(component)
                .ok_or_else(|| anyhow!("History field is missing"))?,
            StoredValue::ObjectRef { id } => {
                let table = tx.open_table(OBJECT_FIELDS)?;
                let row = table
                    .get((conversation, id.as_str(), component.as_str()))?
                    .ok_or_else(|| anyhow!("History field is missing"))?;
                rmp_serde::from_slice(row.value())?
            }
            StoredValue::Array { id, length } => {
                let index: u64 = component
                    .parse()
                    .map_err(|_| anyhow!("History array offset is invalid"))?;
                if index >= length {
                    return Err(anyhow!("History array offset is invalid"));
                }
                let table = tx.open_table(ARRAY_ITEMS)?;
                let row = table
                    .get((conversation, id.as_str(), index))?
                    .ok_or_else(|| anyhow!("History array item is missing"))?;
                rmp_serde::from_slice(row.value())?
            }
            _ => return Err(anyhow!("History field is unavailable")),
        };
    }
    Ok(value)
}

struct Window<'a> {
    tx: &'a redb::ReadTransaction,
    conversation: &'a str,
    cache: &'a parking_lot::Mutex<super::cache::HistoryCache>,
    bytes: usize,
    items: usize,
    more: Vec<HistoryContentCursor>,
    visiting: HashSet<String>,
}

impl Window<'_> {
    fn child(cursor: &HistoryContentCursor, key: String) -> HistoryContentCursor {
        let mut child = cursor.clone();
        child.path.push(key);
        child.offset = 0;
        child.byte_offset = 0;
        child.after_key = None;
        child
    }

    fn read(&mut self, value: StoredValue, mut cursor: HistoryContentCursor) -> Result<Value> {
        Ok(match value {
            StoredValue::Shared { id } => {
                if !self.visiting.insert(id.clone()) {
                    return Err(anyhow!("History tool payload cycle detected"));
                }
                let value = self.read(
                    super::tool_payloads::value(self.tx, self.conversation, &id)?,
                    cursor,
                )?;
                self.visiting.remove(&id);
                value
            }
            StoredValue::JsonText { id, .. } => {
                if !self.visiting.insert(id.clone()) {
                    return Err(anyhow!("History tool payload cycle detected"));
                }
                let value = self.read(
                    super::tool_payloads::value(self.tx, self.conversation, &id)?,
                    cursor,
                )?;
                self.visiting.remove(&id);
                Value::String(serde_json::to_string_pretty(&value)?)
            }
            StoredValue::Scalar(Value::String(text)) => {
                let start = cursor.byte_offset;
                if start > text.len() || !text.is_char_boundary(start) {
                    return Err(anyhow!("History text offset is invalid"));
                }
                let mut end = text.len().min(start.saturating_add(self.bytes));
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                self.bytes -= end - start;
                if end < text.len() {
                    cursor.byte_offset = end;
                    self.more.push(cursor);
                }
                Value::String(text[start..end].to_owned())
            }
            StoredValue::Scalar(value) => value,
            StoredValue::Text { bytes: 0, .. } => Value::String(String::new()),
            StoredValue::Text { id, bytes } => {
                let table = self.tx.open_table(TEXT_CHUNKS)?;
                let key = table
                    .get((self.conversation, id.as_str(), cursor.offset))?
                    .ok_or_else(|| anyhow!("History text offset is invalid"))?;
                let decoded =
                    super::text::chunk(self.tx, self.conversation, key.value(), self.cache)?;
                let text = std::str::from_utf8(&decoded)?;
                let start = cursor.byte_offset;
                if start > text.len() || !text.is_char_boundary(start) {
                    return Err(anyhow!("History text offset is invalid"));
                }
                let mut end = text.len().min(start.saturating_add(self.bytes));
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                self.bytes -= end - start;
                if end < text.len() {
                    cursor.byte_offset = end;
                    self.more.push(cursor);
                } else if cursor.offset + (text.len() as u64) < bytes {
                    cursor.offset += text.len() as u64;
                    cursor.byte_offset = 0;
                    self.more.push(cursor);
                }
                Value::String(text[start..end].to_owned())
            }
            StoredValue::Array { id, length } => {
                let table = self.tx.open_table(ARRAY_ITEMS)?;
                let mut output = Vec::new();
                let mut index = cursor.offset;
                while index < length && self.items > 0 && self.bytes > 0 {
                    self.items -= 1;
                    let row = table
                        .get((self.conversation, id.as_str(), index))?
                        .ok_or_else(|| anyhow!("History array item is missing"))?;
                    output.push(self.read(
                        rmp_serde::from_slice(row.value())?,
                        Self::child(&cursor, index.to_string()),
                    )?);
                    index += 1;
                }
                if index < length {
                    cursor.offset = index;
                    self.more.push(cursor);
                }
                Value::Array(output)
            }
            StoredValue::Object(fields) => {
                let mut output = serde_json::Map::new();
                for (key, value) in fields {
                    if cursor.after_key.as_ref().is_some_and(|after| &key <= after) {
                        continue;
                    }
                    if self.items == 0 || self.bytes < key.len() {
                        self.more.push(cursor);
                        break;
                    }
                    self.items -= 1;
                    self.bytes -= key.len();
                    output.insert(
                        key.clone(),
                        self.read(value, Self::child(&cursor, key.clone()))?,
                    );
                    cursor.after_key = Some(key);
                }
                Value::Object(output)
            }
            StoredValue::ObjectRef { id } => {
                let table = self.tx.open_table(OBJECT_FIELDS)?;
                let mut output = serde_json::Map::new();
                let start = cursor.after_key.clone().unwrap_or_default();
                for row in table.range(
                    (self.conversation, id.as_str(), start.as_str())
                        ..=(self.conversation, id.as_str(), "\u{10ffff}"),
                )? {
                    let (key, value) = row?;
                    let key = key.value().2;
                    if cursor
                        .after_key
                        .as_deref()
                        .is_some_and(|after| key <= after)
                    {
                        continue;
                    }
                    if self.items == 0 || self.bytes < key.len() {
                        self.more.push(cursor.clone());
                        break;
                    }
                    self.items -= 1;
                    self.bytes -= key.len();
                    output.insert(
                        key.to_owned(),
                        self.read(
                            rmp_serde::from_slice(value.value())?,
                            Self::child(&cursor, key.to_owned()),
                        )?,
                    );
                    cursor.after_key = Some(key.to_owned());
                }
                Value::Object(output)
            }
        })
    }
}
