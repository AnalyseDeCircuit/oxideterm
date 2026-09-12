use super::{content, records::*, *};

impl ConversationStore {
    /// Export expands all messages and archive references, independently of UI and model budgets.
    pub fn export_conversation(&self, conversation: &str, branch: &str) -> Result<AiConversation> {
        let guard = self.db.read();
        let tx = guard
            .as_ref()
            .ok_or_else(|| anyhow!("History database is unavailable"))?
            .begin_read()?;
        let mut output = super::records::head(&tx, conversation)?
            .ok_or_else(|| anyhow!("History conversation is missing"))?
            .conversation;
        let mut end = u64::MAX;
        let mut pages = Vec::new();
        loop {
            let keys = super::store::page_keys(&tx, conversation, branch, end, HISTORY_PAGE_SIZE)?;
            if keys.is_empty() {
                break;
            }
            let mut page = Vec::new();
            for (sequence, key) in keys {
                end = sequence;
                page.push(read_message(&tx, conversation, &key, &self.cache)?);
            }
            page.reverse();
            pages.push(page);
        }
        output.messages = pages.into_iter().rev().flatten().collect();
        drop(tx);
        drop(guard);
        for message in &mut output.messages {
            self.expand_message_archives(conversation, message)?;
        }
        output.message_count = output.messages.len();
        output.turn_count = crate::ai_conversation_turn_count(&output.messages);
        output.messages_loaded = true;
        Ok(output)
    }

    pub fn message_by_id(
        &self,
        conversation: &str,
        branch: &str,
        id: &str,
    ) -> Result<AiChatMessage> {
        let guard = self.db.read();
        let tx = guard
            .as_ref()
            .ok_or_else(|| anyhow!("History database is unavailable"))?
            .begin_read()?;
        let (_, key) = message_key(&tx, conversation, branch, id)?;
        read_message(&tx, conversation, &key, &self.cache)
    }

    /// Retry inspects the durable tail and its user boundary without loading earlier bodies.
    pub fn retry_messages(&self, conversation: &str, branch: &str) -> Result<Vec<AiChatMessage>> {
        let guard = self.db.read();
        let tx = guard
            .as_ref()
            .ok_or_else(|| anyhow!("History database is unavailable"))?
            .begin_read()?;
        let descriptions = tx.open_table(DESCRIPTORS)?;
        let mut result = Vec::new();
        let mut end = u64::MAX;
        loop {
            let keys = super::store::page_keys(&tx, conversation, branch, end, HISTORY_PAGE_SIZE)?;
            if keys.is_empty() {
                break;
            }
            for (sequence, key) in keys {
                end = sequence;
                let description: MessageDescriptor = rmp_serde::from_slice(
                    descriptions
                        .get((conversation, key.as_str()))?
                        .ok_or_else(|| anyhow!("History description is missing"))?
                        .value(),
                )?;
                if result.is_empty() || description.role == AiChatRole::User {
                    result.push(read_message(&tx, conversation, &key, &self.cache)?);
                }
                if description.role == AiChatRole::User {
                    result.reverse();
                    return Ok(result);
                }
            }
        }
        result.reverse();
        Ok(result)
    }

    /// Read a coherent model context independently of the pages currently displayed by the UI.
    /// Stop only at a user-turn boundary so a tool result cannot outlive its call.
    pub fn model_context(
        &self,
        conversation: &str,
        branch: &str,
        budget: usize,
        provider: &str,
    ) -> Result<AiConversation> {
        let guard = self.db.read();
        let db = guard
            .as_ref()
            .ok_or_else(|| anyhow!("History database is unavailable"))?;
        let tx = db.begin_read()?;
        let mut result = super::records::head(&tx, conversation)?
            .ok_or_else(|| anyhow!("History conversation is missing"))?
            .conversation;
        let table = tx.open_table(MESSAGES)?;
        let mut end = u64::MAX;
        let mut tokens = 0usize;
        'pages: loop {
            let keys = super::store::page_keys(&tx, conversation, branch, end, 50)?;
            if keys.is_empty() {
                break;
            }
            for (sequence, key) in keys {
                end = sequence;
                let row = table
                    .get((conversation, key.as_str()))?
                    .ok_or_else(|| anyhow!("History message is missing"))?;
                let value = content::load_value(
                    &tx,
                    conversation,
                    rmp_serde::from_slice(row.value())?,
                    &self.cache,
                )?;
                let mut message: AiChatMessage = serde_json::from_value(value)?;
                normalize_interrupted_assistant_projection(&mut message);
                message.is_streaming = false;
                // Alternative tails and archived originals are display history, never additional model input.
                message.branches = None;
                if let Some(metadata) = &mut message.metadata {
                    metadata.original_messages = None;
                }
                let summary = message
                    .metadata
                    .as_ref()
                    .is_some_and(|metadata| metadata.kind == "compaction-anchor")
                    || message
                        .summary_ref
                        .as_ref()
                        .and_then(|value| value.get("kind"))
                        .and_then(Value::as_str)
                        == Some("conversation");
                tokens = tokens.saturating_add(
                    crate::stream_state::ai_message_payload_estimated_tokens(&message, provider),
                );
                let boundary = message.role == AiChatRole::User;
                result.messages.push(message);
                if summary || (tokens >= budget && boundary) {
                    break 'pages;
                }
            }
        }
        result.messages.reverse();
        result.messages_loaded = true;
        Ok(result)
    }
}

fn read_message(
    tx: &redb::ReadTransaction,
    conversation: &str,
    key: &str,
    cache: &parking_lot::Mutex<super::cache::HistoryCache>,
) -> Result<AiChatMessage> {
    let table = tx.open_table(MESSAGES)?;
    let row = table
        .get((conversation, key))?
        .ok_or_else(|| anyhow!("History message is missing"))?;
    let mut message: AiChatMessage = serde_json::from_value(content::load_value(
        tx,
        conversation,
        rmp_serde::from_slice(row.value())?,
        cache,
    )?)?;
    normalize_interrupted_assistant_projection(&mut message);
    message.is_streaming = false;
    Ok(message)
}

pub(super) fn message_key(
    tx: &redb::ReadTransaction,
    conversation: &str,
    branch_id: &str,
    id: &str,
) -> Result<(u64, String)> {
    let ids = tx.open_table(IDS)?;
    let order = tx.open_table(ORDER)?;
    let mut current = branch_id.to_owned();
    let mut end = u64::MAX;
    let mut start = 0;
    let mut visited = HashSet::new();
    loop {
        if !visited.insert(current.clone()) {
            return Err(anyhow!("History branch cycle detected"));
        }
        if tx
            .open_table(MESSAGE_DELETED)?
            .get((conversation, current.as_str(), id))?
            .is_some()
        {
            return Err(anyhow!("History boundary is missing"));
        }
        if let Some(sequence) = ids
            .get((conversation, current.as_str(), id))?
            .map(|row| row.value())
        {
            if sequence >= start && sequence <= end {
                let key = order
                    .get((conversation, current.as_str(), sequence))?
                    .ok_or_else(|| anyhow!("History index is missing"))?
                    .value()
                    .to_owned();
                return Ok((sequence, key));
            }
        }
        let branch = super::records::branch(tx, conversation, &current)?;
        let Some(parent) = branch.parent else {
            return Err(anyhow!("History boundary is missing"));
        };
        current = parent.branch_id;
        start = start.max(parent.first_sequence);
        end = end.min(parent.last_sequence);
    }
}
