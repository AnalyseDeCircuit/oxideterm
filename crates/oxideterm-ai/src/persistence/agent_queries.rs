use super::{records::*, *};

pub fn agent_history_branch(run: &crate::agent::AgentRunId) -> String {
    format!("agent:{run}")
}

pub struct AgentCommunicationPage {
    pub message: crate::agent::AgentMessage,
    pub sequence: u64,
    pub before: Option<u64>,
    pub more: Vec<HistoryContentCursor>,
}

impl ConversationStore {
    /// Communication pages contain one bounded message; older messages and long text remain queryable.
    pub fn agent_communication_page(
        &self,
        conversation: &str,
        run: &crate::agent::AgentRunId,
        before: Option<u64>,
    ) -> Result<Option<AgentCommunicationPage>> {
        let family = format!("agent-communication:{run}");
        let (sequence, older, revision, id, message) = {
            let guard = self.db.read();
            let tx = guard
                .as_ref()
                .ok_or_else(|| anyhow!("History database is unavailable"))?
                .begin_read()?;
            let table = tx.open_table(EVENTS)?;
            let mut rows = table
                .range(
                    (conversation, family.as_str(), 0)
                        ..(conversation, family.as_str(), before.unwrap_or(u64::MAX)),
                )?
                .rev();
            let Some(row) = rows.next() else {
                return Ok(None);
            };
            let (key, value) = row?;
            let sequence = key.value().2;
            let older = rows.next().transpose()?.is_some();
            let mut fields =
                content::object_fields(&tx, conversation, rmp_serde::from_slice(value.value())?)?;
            fields.insert(
                "text".into(),
                StoredValue::Scalar(Value::String(String::new())),
            );
            let message: crate::agent::AgentMessage = serde_json::from_value(content::load_value(
                &tx,
                conversation,
                StoredValue::Object(fields),
                &self.cache,
            )?)?;
            let id = message.sequence.to_string();
            let revision = tx
                .open_table(EVENT_REVISIONS)?
                .get((conversation, family.as_str(), id.as_str()))?
                .ok_or_else(|| anyhow!("History communication version is missing"))?
                .value();
            (sequence, older, revision, id, message)
        };
        let cursor = HistoryContentCursor {
            event: Some(super::windows::HistoryEventLocation {
                family,
                id,
                sequence,
            }),
            conversation_id: conversation.into(),
            storage_id: String::new(),
            revision,
            path: vec!["text".into()],
            offset: 0,
            byte_offset: 0,
            after_key: None,
        };
        let page = self.content_page(&cursor)?;
        let mut message = message;
        message.text = crate::agent::AgentText::new(
            page.value
                .as_str()
                .ok_or_else(|| anyhow!("History communication text is invalid"))?,
        );
        Ok(Some(AgentCommunicationPage {
            message,
            sequence,
            before: older.then_some(sequence),
            more: page.more,
        }))
    }

    pub fn load_agent_summaries(
        &self,
        conversation: &str,
    ) -> Result<Vec<crate::agent::AgentRecord>> {
        let guard = self.db.read();
        let tx = guard
            .as_ref()
            .ok_or_else(|| anyhow!("History database is unavailable"))?
            .begin_read()?;
        let table = tx.open_table(EVENTS)?;
        let mut records = Vec::new();
        for row in table.range((conversation, "agent", 0)..=(conversation, "agent", u64::MAX))? {
            let (_, value) = row?;
            let mut fields =
                content::object_fields(&tx, conversation, rmp_serde::from_slice(value.value())?)?;
            fields.insert(
                "messages".into(),
                StoredValue::Scalar(Value::Array(Vec::new())),
            );
            fields.insert(
                "communication".into(),
                StoredValue::Scalar(Value::Array(Vec::new())),
            );
            let value = StoredValue::Object(fields);
            records.push(serde_json::from_value(content::load_value(
                &tx,
                conversation,
                value,
                &self.cache,
            )?)?);
        }
        Ok(records)
    }

    pub fn load_agent_record(
        &self,
        conversation: &str,
        run: &crate::agent::AgentRunId,
    ) -> Result<Option<crate::agent::AgentRecord>> {
        let guard = self.db.read();
        let tx = guard
            .as_ref()
            .ok_or_else(|| anyhow!("History database is unavailable"))?
            .begin_read()?;
        let ids = tx.open_table(EVENT_IDS)?;
        let run = run.to_string();
        let Some(sequence) = ids.get((conversation, "agent", run.as_str()))? else {
            return Ok(None);
        };
        let table = tx.open_table(EVENTS)?;
        let row = table
            .get((conversation, "agent", sequence.value()))?
            .ok_or_else(|| anyhow!("History agent record is missing"))?;
        let value = content::load_value(
            &tx,
            conversation,
            rmp_serde::from_slice(row.value())?,
            &self.cache,
        )?;
        let branch = value
            .get("messageBranch")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("History agent message reference is missing"))?;
        let mut record: crate::agent::AgentRecord = serde_json::from_value(value.clone())?;
        let table = tx.open_table(MESSAGES)?;
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
                let row = table
                    .get((conversation, key.as_str()))?
                    .ok_or_else(|| anyhow!("History agent message is missing"))?;
                page.push(serde_json::from_value(content::load_value(
                    &tx,
                    conversation,
                    rmp_serde::from_slice(row.value())?,
                    &self.cache,
                )?)?);
            }
            page.reverse();
            pages.push(page);
        }
        record.messages = pages.into_iter().rev().flatten().collect();
        record.communication.clear();
        let family = format!("agent-communication:{run}");
        for row in tx
            .open_table(EVENTS)?
            .range((conversation, family.as_str(), 0)..=(conversation, family.as_str(), u64::MAX))?
        {
            let (_, value) = row?;
            record
                .communication
                .push(serde_json::from_value(content::load_value(
                    &tx,
                    conversation,
                    rmp_serde::from_slice(value.value())?,
                    &self.cache,
                )?)?);
        }
        record
            .communication
            .sort_unstable_by_key(|message| message.sequence);
        for message in &mut record.messages {
            normalize_agent_message(message);
        }
        Ok(Some(record))
    }
}

pub(super) fn normalize_agent_message(message: &mut AiChatMessage) {
    normalize_interrupted_assistant_projection(message);
    message.is_streaming = false;
    for call in &mut message.tool_calls {
        if let Some(object) = call.as_object_mut() {
            object.remove("approvalGeneration");
            if matches!(
                object.get("status").and_then(Value::as_str),
                Some(
                    "running"
                        | "pending"
                        | "pending_user_approval"
                        | "waiting_user"
                        | "pending_user_selection"
                )
            ) {
                object.insert("status".into(), Value::String("rejected".into()));
            }
        }
    }
}
