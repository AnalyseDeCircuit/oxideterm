use super::records::CONTENT_CHUNK_BYTES;
use super::*;

/// A visible projection borrows the live owner and copies at most one bounded section.
pub fn live_message_view(
    message: &AiChatMessage,
    conversation: &str,
    revision: u64,
    section: Option<u64>,
) -> Result<HistoryMessageView> {
    let parts = message
        .turn
        .as_ref()
        .and_then(|turn| turn.get("parts"))
        .and_then(Value::as_array);
    let thinking = message.thinking_content.is_some();
    let sections = parts.filter(|parts| !parts.is_empty()).map_or(
        1 + usize::from(thinking) + message.tool_calls.len(),
        Vec::len,
    ) as u64;
    let section = section
        .unwrap_or(sections.saturating_sub(1))
        .min(sections.saturating_sub(1));
    let mut output = AiChatMessage {
        id: message.id.clone(),
        role: message.role,
        timestamp_ms: message.timestamp_ms,
        content: String::new(),
        model: message.model.clone(),
        context: None,
        thinking_content: None,
        is_streaming: message.is_streaming,
        tool_call_id: message.tool_call_id.clone(),
        tool_calls: Vec::new(),
        turn: None,
        metadata: message
            .metadata
            .as_ref()
            .map(|metadata| crate::AiChatMessageMetadata {
                kind: metadata.kind.clone(),
                original_count: metadata.original_count,
                compacted_at_ms: metadata.compacted_at_ms,
                original_messages: None,
                original_ref: metadata.original_ref.clone(),
                original_user_count: metadata.original_user_count,
            }),
        branches: message
            .branches
            .as_ref()
            .map(|branches| crate::AiMessageBranches {
                total: branches.total,
                active_index: branches.active_index,
                tails: Default::default(),
                refs: branches.refs.clone(),
            }),
        transcript_ref: message.transcript_ref.clone(),
        summary_ref: message.summary_ref.clone(),
        suggestions: message.suggestions.clone(),
    };
    let mut more = Vec::new();
    let mut read = |path: Vec<String>| {
        let cursor = HistoryContentCursor {
            event: None,
            conversation_id: conversation.into(),
            storage_id: message.id.clone(),
            revision,
            path,
            offset: 0,
            byte_offset: 0,
            after_key: None,
        };
        let page = live_content_page(message, &cursor)?;
        more.extend(page.more);
        Ok::<_, anyhow::Error>(page.value)
    };
    if let Some(part) = parts.and_then(|parts| parts.get(section as usize)) {
        let tool_id = part
            .get("toolCallId")
            .or_else(|| part.get("id"))
            .and_then(Value::as_str);
        let call = tool_id.and_then(|id| {
            message
                .tool_calls
                .iter()
                .position(|call| call.get("id").and_then(Value::as_str) == Some(id))
        });
        if let Some(index) = call {
            output
                .tool_calls
                .push(read(vec!["tool_calls".into(), index.to_string()])?);
        } else {
            output.turn = Some(
                serde_json::json!({"parts":[read(vec!["turn".into(), "parts".into(), section.to_string()])?]}),
            );
        }
    } else if section == 0 {
        output.content = read(vec!["content".into()])?
            .as_str()
            .ok_or_else(|| anyhow!("Live text is invalid"))?
            .to_owned();
    } else if thinking && section == 1 {
        output.thinking_content = Some(
            read(vec!["thinking_content".into()])?
                .as_str()
                .ok_or_else(|| anyhow!("Live thinking is invalid"))?
                .to_owned(),
        );
    } else {
        output.tool_calls.push(read(vec![
            "tool_calls".into(),
            (section - 1 - u64::from(thinking)).to_string(),
        ])?);
    }
    Ok(HistoryMessageView {
        message: output,
        section,
        sections,
        more,
    })
}

pub fn live_content_page(
    message: &AiChatMessage,
    cursor: &HistoryContentCursor,
) -> Result<HistoryContentPage> {
    if cursor.storage_id != message.id {
        return Err(anyhow!("Live content owner changed"));
    }
    let mut window = LiveWindow {
        bytes: CONTENT_CHUNK_BYTES,
        items: HISTORY_PAGE_SIZE,
        more: Vec::new(),
    };
    let value = match cursor.path.first().map(String::as_str) {
        Some("content") if cursor.path.len() == 1 => {
            window.text(&message.content, cursor.clone())?
        }
        Some("thinking_content") if cursor.path.len() == 1 => window.text(
            message.thinking_content.as_deref().unwrap_or_default(),
            cursor.clone(),
        )?,
        Some("turn") => {
            let mut value = message
                .turn
                .as_ref()
                .ok_or_else(|| anyhow!("Live turn is missing"))?;
            for key in &cursor.path[1..] {
                value = child(value, key)?;
            }
            window.value(value, cursor.clone())?
        }
        Some("tool_calls") => {
            let index: usize = cursor
                .path
                .get(1)
                .ok_or_else(|| anyhow!("Live tool index is missing"))?
                .parse()?;
            let mut value = message
                .tool_calls
                .get(index)
                .ok_or_else(|| anyhow!("Live tool is missing"))?;
            for key in &cursor.path[2..] {
                value = child(value, key)?;
            }
            window.value(value, cursor.clone())?
        }
        _ => return Err(anyhow!("Live content path is invalid")),
    };
    Ok(HistoryContentPage {
        value,
        more: window.more,
    })
}
fn child<'a>(value: &'a Value, key: &str) -> Result<&'a Value> {
    let value = match value {
        Value::Array(values) => values.get(key.parse::<usize>()?),
        _ => value.get(key),
    };
    value.ok_or_else(|| anyhow!("Live content changed"))
}
struct LiveWindow {
    bytes: usize,
    items: usize,
    more: Vec<HistoryContentCursor>,
}
impl LiveWindow {
    fn text(&mut self, text: &str, mut cursor: HistoryContentCursor) -> Result<Value> {
        let start = cursor.byte_offset;
        if start > text.len() || !text.is_char_boundary(start) {
            return Err(anyhow!("Live text boundary changed"));
        }
        let mut end = start.saturating_add(self.bytes).min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        self.bytes -= end - start;
        let value = Value::String(text[start..end].into());
        if end < text.len() {
            cursor.byte_offset = end;
            self.more.push(cursor);
        }
        Ok(value)
    }
    fn value(&mut self, value: &Value, cursor: HistoryContentCursor) -> Result<Value> {
        match value {
            Value::String(text) => self.text(text, cursor),
            Value::Array(values) => {
                let mut output = Vec::new();
                for (index, value) in values.iter().enumerate().skip(cursor.offset as usize) {
                    if self.items == 0 || self.bytes == 0 {
                        let mut next = cursor.clone();
                        next.offset = index as u64;
                        self.more.push(next);
                        break;
                    }
                    self.items -= 1;
                    let mut next = cursor.clone();
                    next.path.push(index.to_string());
                    next.offset = 0;
                    next.byte_offset = 0;
                    next.after_key = None;
                    output.push(self.value(value, next)?);
                }
                Ok(Value::Array(output))
            }
            Value::Object(values) => {
                let mut output = serde_json::Map::new();
                for (index, (key, value)) in values.iter().enumerate().skip(cursor.offset as usize)
                {
                    if self.items == 0 || self.bytes == 0 {
                        let mut next = cursor.clone();
                        next.offset = index as u64;
                        self.more.push(next);
                        break;
                    }
                    self.items -= 1;
                    let mut next = cursor.clone();
                    next.path.push(key.clone());
                    next.offset = 0;
                    next.byte_offset = 0;
                    next.after_key = None;
                    output.insert(key.clone(), self.value(value, next)?);
                }
                for key in ["type", "id", "toolCallId", "name", "status"] {
                    if let Some(value) = values.get(key) {
                        output.insert(key.into(), value.clone());
                    }
                }
                Ok(Value::Object(output))
            }
            _ => Ok(value.clone()),
        }
    }
}
