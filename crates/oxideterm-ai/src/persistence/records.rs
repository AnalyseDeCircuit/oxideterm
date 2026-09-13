use super::*;
use std::collections::BTreeMap;

pub const HISTORY_PAGE_SIZE: usize = 50;
pub const HISTORY_CACHE_BYTES: usize = 64 * 1024 * 1024;
pub const CONTENT_CHUNK_BYTES: usize = 64 * 1024;

pub(super) const HEADS: TableDefinition<&str, &[u8]> = TableDefinition::new("v4_conversations");
pub(super) const BRANCHES: TableDefinition<(&str, &str), &[u8]> =
    TableDefinition::new("v4_branches");
pub(super) const ORDER: TableDefinition<(&str, &str, u64), &str> =
    TableDefinition::new("v4_message_order");
pub(super) const IDS: TableDefinition<(&str, &str, &str), u64> =
    TableDefinition::new("v4_message_ids");
pub(super) const DESCRIPTORS: TableDefinition<(&str, &str), &[u8]> =
    TableDefinition::new("v4_message_descriptors");
pub(super) const MESSAGES: TableDefinition<(&str, &str), &[u8]> =
    TableDefinition::new("v4_messages");
pub(super) const BLOBS: TableDefinition<(&str, &str), &[u8]> = TableDefinition::new("v4_content");
pub(super) const ARRAY_ITEMS: TableDefinition<(&str, &str, u64), &[u8]> =
    TableDefinition::new("v4_array_items");
pub(super) const OBJECT_FIELDS: TableDefinition<(&str, &str, &str), &[u8]> =
    TableDefinition::new("v4_object_fields");
pub(super) const TOOL_PARTS: TableDefinition<(&str, &str, &str), &[u8]> =
    TableDefinition::new("v4_tool_parts");
pub(super) const TEXT_CHUNKS: TableDefinition<(&str, &str, u64), &str> =
    TableDefinition::new("v4_text_chunks");
pub(super) const HISTORY_REFS: TableDefinition<(&str, &str, u64), &[u8]> =
    TableDefinition::new("v4_history_refs");
pub(super) const BRANCH_DELETED: TableDefinition<(&str, &str), u64> =
    TableDefinition::new("v4_branch_deleted");
pub(super) const PAYLOADS: TableDefinition<(&str, &str), &[u8]> =
    TableDefinition::new("v4_tool_payloads");
pub(super) const PAYLOAD_REFS: TableDefinition<(&str, &str), u64> =
    TableDefinition::new("v4_tool_payload_refs");
pub(super) const REFS: TableDefinition<(&str, &str), u64> = TableDefinition::new("v4_content_refs");
pub(super) const MESSAGE_DELETED: TableDefinition<(&str, &str, &str), u64> =
    TableDefinition::new("v4_message_deleted");
pub(super) const EVENT_REVISIONS: TableDefinition<(&str, &str, &str), u64> =
    TableDefinition::new("v4_event_revisions");
pub(super) const DELETED: TableDefinition<&str, u64> = TableDefinition::new("v4_deleted");
pub(super) const STATE: TableDefinition<&str, u64> = TableDefinition::new("v4_state");
pub(super) const EVENTS: TableDefinition<(&str, &str, u64), &[u8]> =
    TableDefinition::new("v4_events");
pub(super) const EVENT_IDS: TableDefinition<(&str, &str, &str), u64> =
    TableDefinition::new("v4_event_ids");
pub(super) const EVENT_NEXT: TableDefinition<(&str, &str), u64> =
    TableDefinition::new("v4_event_next");
pub(super) const UPDATED: TableDefinition<(i64, &str), ()> = TableDefinition::new("v4_updated");

pub(super) const ARCHIVED_UPDATED: TableDefinition<(u8, i64, &str), ()> =
    TableDefinition::new("v4_archived_updated");

pub(super) fn ensure_conversation_index(db: &Database) -> Result<()> {
    if db
        .begin_read()?
        .open_table(STATE)?
        .get("conversation_index")?
        .is_some()
    {
        return Ok(());
    }
    let tx = db.begin_write()?;
    let mut revision = 0;
    {
        let mut index = tx.open_table(ARCHIVED_UPDATED)?;
        for row in tx.open_table(HEADS)?.iter()? {
            let (_, value) = row?;
            let head: ConversationHead = rmp_serde::from_slice(value.value())?;
            index.insert(
                (
                    u8::from(head.conversation.archived),
                    head.conversation.updated_at_ms,
                    head.conversation.id.as_str(),
                ),
                (),
            )?;
            revision = revision.max(head.revision);
        }
        for row in tx.open_table(DELETED)?.iter()? {
            revision = revision.max(row?.1.value());
        }
    }
    tx.open_table(STATE)?.insert("max_revision", revision)?;
    tx.open_table(STATE)?.insert("conversation_index", 1)?;
    tx.commit()?;
    Ok(())
}

pub(super) fn record_revision(tx: &redb::WriteTransaction, revision: u64) -> Result<()> {
    let mut state = tx.open_table(STATE)?;
    let previous = state
        .get("max_revision")?
        .map(|v| v.value())
        .unwrap_or_default();
    if revision > previous {
        state.insert("max_revision", revision)?;
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConversationHead {
    pub conversation: AiConversation,
    pub revision: u64,
    pub metadata_revision: u64,
    pub title_revision: u64,
    pub structure_revision: u64,
    pub active_branch: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Branch {
    pub parent: Option<BranchParent>,
    pub owner: BranchOwner,
    pub retained: bool,
    pub sealed: bool,
    pub next_sequence: u64,
    pub message_count: usize,
    pub turn_count: usize,
    pub revision: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) enum BranchOwner {
    Conversation,
    Archive,
    Agent,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct BranchParent {
    pub branch_id: String,
    pub first_sequence: u64,
    pub last_sequence: u64,
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

#[derive(Clone, Serialize, Deserialize)]
pub(super) enum StoredValue {
    Scalar(Value),
    Text { id: String, bytes: u64 },
    Array { id: String, length: u64 },
    Object(BTreeMap<String, StoredValue>),
    ObjectRef { id: String },
    Shared { id: String },
    JsonText { id: String, order: Option<String> },
}

pub(super) fn initialize(db: &Database) -> Result<()> {
    let tx = db.begin_write()?;
    tx.open_table(HEADS)?;
    tx.open_table(BRANCHES)?;
    tx.open_table(ORDER)?;
    tx.open_table(IDS)?;
    tx.open_table(MESSAGES)?;
    tx.open_table(DESCRIPTORS)?;
    tx.open_table(BLOBS)?;
    tx.open_table(ARRAY_ITEMS)?;
    tx.open_table(OBJECT_FIELDS)?;
    tx.open_table(TOOL_PARTS)?;
    tx.open_table(TEXT_CHUNKS)?;
    tx.open_table(HISTORY_REFS)?;
    tx.open_table(BRANCH_DELETED)?;
    tx.open_table(PAYLOADS)?;
    tx.open_table(PAYLOAD_REFS)?;
    tx.open_table(REFS)?;
    tx.open_table(DELETED)?;
    tx.open_table(MESSAGE_DELETED)?;
    tx.open_table(EVENT_REVISIONS)?;
    tx.open_table(STATE)?.insert("version", 4)?;
    tx.open_table(EVENTS)?;
    tx.open_table(EVENT_IDS)?;
    tx.open_table(EVENT_NEXT)?;
    tx.open_table(UPDATED)?;
    tx.open_table(ARCHIVED_UPDATED)?;
    tx.open_table(STATE)?.insert("conversation_index", 1)?;
    tx.open_table(STATE)?.insert("max_revision", 0)?;
    tx.commit()?;
    Ok(())
}

pub(super) fn head(tx: &redb::ReadTransaction, id: &str) -> Result<Option<ConversationHead>> {
    tx.open_table(HEADS)?
        .get(id)?
        .map(|row| rmp_serde::from_slice(row.value()).map_err(Into::into))
        .transpose()
}

pub(super) fn branch(tx: &redb::ReadTransaction, conversation: &str, id: &str) -> Result<Branch> {
    let table = tx.open_table(BRANCHES)?;
    let row = table
        .get((conversation, id))?
        .ok_or_else(|| anyhow!("History branch is unavailable"))?;
    Ok(rmp_serde::from_slice(row.value())?)
}
