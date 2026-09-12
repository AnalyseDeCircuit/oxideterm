use super::*;
use crate::agent::{AgentRecord, AgentRunId};

pub(super) const AGENT_RECORDS: TableDefinition<&str, &[u8]> =
    TableDefinition::new("agent_records");
const AGENT_SUMMARIES: TableDefinition<&str, &[u8]> = TableDefinition::new("agent_summaries");

pub(super) fn initialize(transaction: &redb::WriteTransaction) -> Result<()> {
    transaction.open_table(AGENT_RECORDS)?;
    transaction.open_table(AGENT_SUMMARIES)?;
    Ok(())
}

impl AiChatPersistenceStore {
    pub fn save_agent_records(&self, records: Vec<AgentRecord>) -> Result<()> {
        let transaction = self.db.begin_write()?;
        {
            let conversations = transaction.open_table(CONVERSATIONS_TABLE)?;
            let mut details = transaction.open_table(AGENT_RECORDS)?;
            let mut summaries = transaction.open_table(AGENT_SUMMARIES)?;
            for mut record in records {
                // A delayed worker cannot recreate tasks belonging to a deleted conversation.
                if conversations
                    .get(record.snapshot.conversation_id.as_str())?
                    .is_none()
                {
                    continue;
                }
                let key = format!(
                    "{}:{}",
                    record.snapshot.conversation_id, record.snapshot.run.run_id
                );
                if let Some(stored) = summaries.get(key.as_str())? {
                    let previous: AgentRecord = rmp_serde::from_slice(stored.value())?;
                    if previous.revision > record.revision {
                        continue;
                    }
                }
                for message in &mut record.messages {
                    crate::context_sanitizer::sanitize_chat_message_for_persistence(message);
                    message.thinking_content = None;
                    if let Some(parts) = message
                        .turn
                        .as_mut()
                        .and_then(|turn| turn.get_mut("parts"))
                        .and_then(serde_json::Value::as_array_mut)
                    {
                        parts.retain(|part| {
                            part.get("type").and_then(serde_json::Value::as_str) != Some("thinking")
                        });
                    }
                }
                let bytes = rmp_serde::to_vec_named(&record)?;
                details.insert(key.as_str(), bytes.as_slice())?;
                record.messages.clear();
                record.communication.clear();
                let bytes = rmp_serde::to_vec_named(&record)?;
                summaries.insert(key.as_str(), bytes.as_slice())?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn load_agent_summaries(&self, conversation_id: &str) -> Result<Vec<AgentRecord>> {
        let transaction = self.db.begin_read()?;
        let table = transaction.open_table(AGENT_SUMMARIES)?;
        let prefix = format!("{conversation_id}:");
        let mut records = Vec::new();
        for entry in table.range(prefix.as_str()..)? {
            let (key, bytes) = entry?;
            if !key.value().starts_with(&prefix) {
                break;
            }
            records.push(rmp_serde::from_slice(bytes.value())?);
        }
        Ok(records)
    }

    pub fn load_agent_record(
        &self,
        conversation_id: &str,
        run_id: &AgentRunId,
    ) -> Result<Option<AgentRecord>> {
        let transaction = self.db.begin_read()?;
        let table = transaction.open_table(AGENT_RECORDS)?;
        let key = format!("{conversation_id}:{run_id}");
        let mut record: Option<AgentRecord> = table
            .get(key.as_str())?
            .map(|record| rmp_serde::from_slice(record.value()).map_err(anyhow::Error::from))
            .transpose()?;
        if let Some(record) = record.as_mut() {
            for message in &mut record.messages {
                message.is_streaming = false;
                for call in &mut message.tool_calls {
                    if let Some(object) = call.as_object_mut() {
                        object.remove("approvalGeneration");
                        if matches!(
                            object.get("status").and_then(serde_json::Value::as_str),
                            Some(
                                "running"
                                    | "pending"
                                    | "pending_user_approval"
                                    | "waiting_user"
                                    | "pending_user_selection"
                            )
                        ) {
                            object.insert("status".into(), serde_json::json!("rejected"));
                        }
                    }
                }
            }
        }
        Ok(record)
    }
}

pub(super) fn delete_conversation(
    transaction: &redb::WriteTransaction,
    conversation_id: &str,
) -> Result<()> {
    let prefix = format!("{conversation_id}:");
    for definition in [AGENT_RECORDS, AGENT_SUMMARIES] {
        let mut table = transaction.open_table(definition)?;
        let mut keys = Vec::new();
        for entry in table.range(prefix.as_str()..)? {
            let (key, _) = entry?;
            if !key.value().starts_with(&prefix) {
                break;
            }
            keys.push(key.value().to_owned());
        }
        for key in keys {
            table.remove(key.as_str())?;
        }
    }
    Ok(())
}
