use std::{
    collections::HashSet,
    fmt,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result, anyhow};
use base64::{Engine as _, engine::general_purpose};
use redb::{Database, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    AiChatMessage, AiChatMessageMetadata, AiChatRole, AiChatState, AiConversation,
    AiMessageBranches,
};

pub const AI_CHAT_DB_VERSION: u32 = 3;
pub const MAX_CONVERSATIONS: usize = 100;
pub const MAX_MESSAGES_PER_CONVERSATION: usize = 200;

const COMPRESSION_THRESHOLD: usize = 4096;
const ANCHOR_META_HEADER: &str = "$$ANCHOR_B64$$";

const CONVERSATIONS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("conversations");
const MESSAGES_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("messages");
const CONV_MESSAGES_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("conversation_messages");
const TRANSCRIPT_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("ai_chat_transcript");
const CONV_TRANSCRIPT_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("conversation_transcript");
const METADATA_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("ai_chat_metadata");
const DIAGNOSTIC_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("ai_chat_diagnostic_events");
const CONV_DIAGNOSTIC_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("conversation_diagnostic_events");

#[derive(Clone)]
pub struct AiChatPersistenceStore {
    path: PathBuf,
    db: Arc<Database>,
}

impl fmt::Debug for AiChatPersistenceStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AiChatPersistenceStore")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl AiChatPersistenceStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        Self::try_new(path.clone()).unwrap_or_else(|error| {
            panic!("failed to open AI chat redb {}: {error}", path.display())
        })
    }

    pub fn try_new(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let db = open_ai_chat_database(&path)?;
        let store = Self {
            path,
            db: Arc::new(db),
        };
        store.initialize()?;
        Ok(store)
    }

    pub fn load(path: impl Into<PathBuf>) -> Result<(Self, AiChatState)> {
        let store = Self::try_new(path)?;
        let state = store.load_state()?;
        Ok((store, state))
    }

    pub fn load_state(&self) -> Result<AiChatState> {
        self.initialize()?;
        let metas = self.list_conversations()?;
        let active_conversation_id = metas.first().map(|meta| meta.id.clone());
        let mut conversations = metas
            .into_iter()
            .map(conversation_from_meta)
            .collect::<Vec<_>>();

        if let Some(active_id) = active_conversation_id.as_deref()
            && let Some(full) = self.load_conversation(active_id)?
            && let Some(slot) = conversations
                .iter_mut()
                .find(|conversation| conversation.id == active_id)
        {
            *slot = full;
        }

        Ok(AiChatState {
            conversations,
            active_conversation_id,
        })
    }

    pub fn load_conversation(&self, conversation_id: &str) -> Result<Option<AiConversation>> {
        self.initialize()?;
        let read_txn = self.db.begin_read()?;
        let conv_table = read_txn.open_table(CONVERSATIONS_TABLE)?;
        let Some(meta_bytes) = conv_table.get(conversation_id)? else {
            return Ok(None);
        };
        let meta: ConversationMeta = rmp_serde::from_slice(meta_bytes.value())?;

        let index_table = read_txn.open_table(CONV_MESSAGES_TABLE)?;
        let message_ids = index_table
            .get(conversation_id)?
            .map(|bytes| rmp_serde::from_slice::<Vec<String>>(bytes.value()))
            .transpose()?
            .unwrap_or_default();

        let message_table = read_txn.open_table(MESSAGES_TABLE)?;
        let mut messages = Vec::new();
        for message_id in message_ids {
            if let Some(message_bytes) = message_table.get(message_id.as_str())? {
                let mut persisted: PersistedMessage = rmp_serde::from_slice(message_bytes.value())?;
                if let Some(context) = persisted.context_snapshot.as_mut()
                    && let Some(buffer_tail) = context.buffer_tail.as_ref()
                {
                    context.buffer_tail =
                        Some(decompress_buffer(buffer_tail, context.buffer_compressed)?);
                    context.buffer_compressed = false;
                }
                messages.push(message_from_persisted(persisted));
            }
        }
        messages.sort_by(|left, right| left.timestamp_ms.cmp(&right.timestamp_ms));

        let mut conversation = conversation_from_meta(meta);
        conversation.messages = messages;
        conversation.message_count = conversation.messages.len();
        conversation.messages_loaded = true;
        Ok(Some(conversation))
    }

    pub fn save_state(&self, state: &AiChatState) -> Result<()> {
        self.initialize()?;
        let write_txn = self.db.begin_write()?;

        {
            let mut conv_table = write_txn.open_table(CONVERSATIONS_TABLE)?;
            let mut message_table = write_txn.open_table(MESSAGES_TABLE)?;
            let mut message_index_table = write_txn.open_table(CONV_MESSAGES_TABLE)?;
            let mut transcript_table = write_txn.open_table(TRANSCRIPT_TABLE)?;
            let mut transcript_index_table = write_txn.open_table(CONV_TRANSCRIPT_TABLE)?;
            let mut diagnostic_table = write_txn.open_table(DIAGNOSTIC_TABLE)?;
            let mut diagnostic_index_table = write_txn.open_table(CONV_DIAGNOSTIC_TABLE)?;

            let desired_ids = state
                .conversations
                .iter()
                .map(|conversation| conversation.id.clone())
                .collect::<HashSet<_>>();
            let existing_ids = collect_keys(&conv_table)?;
            for conversation_id in existing_ids {
                if !desired_ids.contains(&conversation_id) {
                    delete_conversation_rows(
                        &conversation_id,
                        &mut conv_table,
                        &mut message_table,
                        &mut message_index_table,
                        &mut transcript_table,
                        &mut transcript_index_table,
                        &mut diagnostic_table,
                        &mut diagnostic_index_table,
                    )?;
                }
            }

            for conversation in &state.conversations {
                let mut meta = meta_from_conversation(conversation);
                if !conversation.messages_loaded
                    && let Some(existing) = conv_table.get(conversation.id.as_str())?
                    && let Ok(existing_meta) =
                        rmp_serde::from_slice::<ConversationMeta>(existing.value())
                {
                    meta.message_count = existing_meta.message_count;
                    meta.updated_at = meta.updated_at.max(existing_meta.updated_at);
                }
                let meta_bytes = rmp_serde::to_vec(&meta)?;
                conv_table.insert(conversation.id.as_str(), meta_bytes.as_slice())?;

                ensure_index_row(&mut message_index_table, &conversation.id)?;
                ensure_index_row(&mut transcript_index_table, &conversation.id)?;
                ensure_index_row(&mut diagnostic_index_table, &conversation.id)?;

                if conversation.messages_loaded {
                    replace_conversation_messages(
                        conversation,
                        &mut conv_table,
                        &mut message_table,
                        &mut message_index_table,
                    )?;
                }
            }
        }

        write_txn.commit()?;
        Ok(())
    }

    fn initialize(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create AI chat redb directory {}",
                    parent.display()
                )
            })?;
        }

        let write_txn = self.db.begin_write()?;
        {
            let _ = write_txn.open_table(CONVERSATIONS_TABLE)?;
            let _ = write_txn.open_table(MESSAGES_TABLE)?;
            let _ = write_txn.open_table(CONV_MESSAGES_TABLE)?;
            let _ = write_txn.open_table(TRANSCRIPT_TABLE)?;
            let _ = write_txn.open_table(CONV_TRANSCRIPT_TABLE)?;
            let _ = write_txn.open_table(METADATA_TABLE)?;
            let _ = write_txn.open_table(DIAGNOSTIC_TABLE)?;
            let _ = write_txn.open_table(CONV_DIAGNOSTIC_TABLE)?;
        }
        write_txn.commit()?;

        let write_txn = self.db.begin_write()?;
        {
            let mut metadata = write_txn.open_table(METADATA_TABLE)?;
            let version = metadata
                .get("version")?
                .and_then(|value| rmp_serde::from_slice::<u32>(value.value()).ok());
            if version.is_none_or(|version| version < AI_CHAT_DB_VERSION) {
                let bytes = rmp_serde::to_vec(&AI_CHAT_DB_VERSION)?;
                metadata.insert("version", bytes.as_slice())?;
            }
        }
        write_txn.commit()?;
        Ok(())
    }

    fn list_conversations(&self) -> Result<Vec<ConversationMeta>> {
        let read_txn = self.db.begin_read()?;
        let conv_table = read_txn.open_table(CONVERSATIONS_TABLE)?;
        let mut conversations = Vec::new();
        let mut total_rows = 0usize;
        let mut failed_rows = 0usize;

        for row in conv_table.iter()? {
            total_rows += 1;
            let (key, value) = row?;
            match rmp_serde::from_slice::<ConversationMeta>(value.value()) {
                Ok(meta) => conversations.push(meta),
                Err(error) => {
                    failed_rows += 1;
                    eprintln!(
                        "[AiChatStore] Failed to deserialize conversation '{}': {error}",
                        key.value()
                    );
                }
            }
        }
        if total_rows > 0 && total_rows == failed_rows {
            return Err(anyhow!("all conversations failed to deserialize"));
        }
        conversations.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        conversations.truncate(MAX_CONVERSATIONS);
        Ok(conversations)
    }
}

fn open_ai_chat_database(path: &Path) -> Result<Database> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create AI chat redb directory {}",
                parent.display()
            )
        })?;
    }
    match Database::create(path) {
        Ok(db) => {
            set_owner_only_permissions(path);
            Ok(db)
        }
        Err(redb::DatabaseError::Storage(redb::StorageError::Corrupted(message))) => {
            let backup_path = path.with_extension("redb.backup");
            let _ = std::fs::rename(path, &backup_path);
            eprintln!(
                "[AiChatStore] AI chat redb was corrupted ({message}); backed up to {}",
                backup_path.display()
            );
            let db = Database::create(path)?;
            set_owner_only_permissions(path);
            Ok(db)
        }
        Err(error) => Err(error.into()),
    }
}

#[cfg(unix)]
fn set_owner_only_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn set_owner_only_permissions(_path: &Path) {}

fn collect_keys(table: &redb::Table<'_, &str, &[u8]>) -> Result<Vec<String>, redb::StorageError> {
    table
        .iter()?
        .map(|row| row.map(|(key, _)| key.value().to_string()))
        .collect()
}

fn ensure_index_row(table: &mut redb::Table<'_, &str, &[u8]>, conversation_id: &str) -> Result<()> {
    if table.get(conversation_id)?.is_none() {
        let empty: Vec<String> = Vec::new();
        let bytes = rmp_serde::to_vec(&empty)?;
        table.insert(conversation_id, bytes.as_slice())?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn delete_conversation_rows(
    conversation_id: &str,
    conv_table: &mut redb::Table<'_, &str, &[u8]>,
    message_table: &mut redb::Table<'_, &str, &[u8]>,
    message_index_table: &mut redb::Table<'_, &str, &[u8]>,
    transcript_table: &mut redb::Table<'_, &str, &[u8]>,
    transcript_index_table: &mut redb::Table<'_, &str, &[u8]>,
    diagnostic_table: &mut redb::Table<'_, &str, &[u8]>,
    diagnostic_index_table: &mut redb::Table<'_, &str, &[u8]>,
) -> Result<()> {
    if let Some(list) = message_index_table.get(conversation_id)? {
        for message_id in rmp_serde::from_slice::<Vec<String>>(list.value())? {
            let _ = message_table.remove(message_id.as_str())?;
        }
    }
    let _ = message_index_table.remove(conversation_id)?;

    if let Some(list) = transcript_index_table.get(conversation_id)? {
        for entry_id in rmp_serde::from_slice::<Vec<String>>(list.value())? {
            let _ = transcript_table.remove(entry_id.as_str())?;
        }
    }
    let _ = transcript_index_table.remove(conversation_id)?;

    if let Some(list) = diagnostic_index_table.get(conversation_id)? {
        for event_id in rmp_serde::from_slice::<Vec<String>>(list.value())? {
            let _ = diagnostic_table.remove(event_id.as_str())?;
        }
    }
    let _ = diagnostic_index_table.remove(conversation_id)?;
    let _ = conv_table.remove(conversation_id)?;
    Ok(())
}

fn replace_conversation_messages(
    conversation: &AiConversation,
    conv_table: &mut redb::Table<'_, &str, &[u8]>,
    message_table: &mut redb::Table<'_, &str, &[u8]>,
    message_index_table: &mut redb::Table<'_, &str, &[u8]>,
) -> Result<()> {
    let current_ids = message_index_table
        .get(conversation.id.as_str())?
        .map(|bytes| rmp_serde::from_slice::<Vec<String>>(bytes.value()))
        .transpose()?
        .unwrap_or_default();
    let new_ids = conversation
        .messages
        .iter()
        .rev()
        .take(MAX_MESSAGES_PER_CONVERSATION)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|message| message.id.clone())
        .collect::<Vec<_>>();
    let retained = new_ids.iter().cloned().collect::<HashSet<_>>();
    for old_id in current_ids {
        if !retained.contains(&old_id) {
            let _ = message_table.remove(old_id.as_str())?;
        }
    }
    for message in conversation
        .messages
        .iter()
        .rev()
        .take(MAX_MESSAGES_PER_CONVERSATION)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        let persisted = persisted_from_message(&conversation.id, message);
        let should_write = message_table
            .get(persisted.id.as_str())?
            .map(|bytes| rmp_serde::from_slice::<PersistedMessage>(bytes.value()))
            .transpose()?
            .map(|existing| should_replace_projection(&persisted, &existing))
            .unwrap_or(true);
        if should_write {
            let mut persisted = persisted;
            if let Some(context) = persisted.context_snapshot.as_mut()
                && let Some(buffer) = context.buffer_tail.as_ref()
            {
                let (compressed, compressed_flag) = compress_buffer(buffer);
                context.buffer_tail = Some(compressed);
                context.buffer_compressed = compressed_flag;
            }
            let bytes = rmp_serde::to_vec(&persisted)?;
            message_table.insert(persisted.id.as_str(), bytes.as_slice())?;
        }
    }
    let index_bytes = rmp_serde::to_vec(&new_ids)?;
    message_index_table.insert(conversation.id.as_str(), index_bytes.as_slice())?;

    let mut meta = meta_from_conversation(conversation);
    meta.message_count = new_ids.len();
    let bytes = rmp_serde::to_vec(&meta)?;
    conv_table.insert(conversation.id.as_str(), bytes.as_slice())?;
    Ok(())
}

fn conversation_from_meta(meta: ConversationMeta) -> AiConversation {
    let profile_id = meta
        .session_metadata
        .as_ref()
        .and_then(|metadata| metadata.get("profileId"))
        .and_then(Value::as_str)
        .map(str::to_string);
    AiConversation {
        id: meta.id,
        title: meta.title,
        messages: Vec::new(),
        created_at_ms: meta.created_at,
        updated_at_ms: meta.updated_at,
        origin: meta.origin,
        profile_id,
        message_count: meta.message_count,
        session_id: meta.session_id,
        session_metadata: meta.session_metadata,
        messages_loaded: false,
    }
}

fn meta_from_conversation(conversation: &AiConversation) -> ConversationMeta {
    let session_metadata = conversation.session_metadata.clone().or_else(|| {
        let mut metadata = serde_json::Map::new();
        metadata.insert(
            "conversationId".to_string(),
            Value::String(conversation.id.clone()),
        );
        metadata.insert(
            "origin".to_string(),
            Value::String(conversation.origin.clone()),
        );
        if let Some(profile_id) = conversation.profile_id.as_ref() {
            metadata.insert("profileId".to_string(), Value::String(profile_id.clone()));
        }
        Some(Value::Object(metadata))
    });
    ConversationMeta {
        id: conversation.id.clone(),
        title: conversation.title.clone(),
        created_at: conversation.created_at_ms,
        updated_at: conversation.updated_at_ms,
        message_count: if conversation.messages_loaded {
            conversation.messages.len()
        } else {
            conversation.message_count
        },
        session_id: conversation.session_id.clone(),
        origin: if conversation.origin.is_empty() {
            default_origin()
        } else {
            conversation.origin.clone()
        },
        session_metadata,
    }
}

fn message_from_persisted(message: PersistedMessage) -> AiChatMessage {
    let (content, metadata) =
        decode_anchor_content(&message.content).unwrap_or((message.content, None));
    let thinking_content = message
        .turn
        .as_ref()
        .and_then(thinking_content_from_turn)
        .or_else(|| parse_thinking_tags(&content).map(|parsed| parsed.1));
    let content = if thinking_content.is_some() {
        parse_thinking_tags(&content)
            .map(|parsed| parsed.0)
            .unwrap_or(content)
    } else {
        content
    };
    AiChatMessage {
        id: message.id,
        role: role_from_str(&message.role),
        content,
        timestamp_ms: message.timestamp,
        model: message.model,
        context: message
            .context_snapshot
            .and_then(|context| context.buffer_tail),
        thinking_content,
        is_streaming: false,
        metadata,
        tool_call_id: message.tool_call_id,
        tool_calls: message
            .tool_calls
            .into_iter()
            .map(|call| serde_json::to_value(call).unwrap_or(Value::Null))
            .collect(),
        turn: message.turn,
        transcript_ref: message.transcript_ref,
        summary_ref: message.summary_ref,
        branches: message.branches,
    }
}

fn persisted_from_message(conversation_id: &str, message: &AiChatMessage) -> PersistedMessage {
    let mut turn = message.turn.clone();
    if message.role == AiChatRole::Assistant && turn.is_none() && message.thinking_content.is_some()
    {
        turn = Some(legacy_turn_from_message(message));
    }
    PersistedMessage {
        id: message.id.clone(),
        conversation_id: conversation_id.to_string(),
        role: role_to_str(message.role).to_string(),
        content: if let Some(metadata) = message.metadata.as_ref() {
            encode_anchor_content(&message.content, metadata)
        } else {
            message.content.clone()
        },
        timestamp: message.timestamp_ms,
        projection_updated_at: message.timestamp_ms,
        tool_calls: message
            .tool_calls
            .iter()
            .filter_map(|value| serde_json::from_value::<PersistedToolCall>(value.clone()).ok())
            .collect(),
        tool_call_id: message.tool_call_id.clone(),
        context_snapshot: message.context.as_ref().map(|context| ContextSnapshot {
            cwd: None,
            selection: None,
            buffer_tail: Some(context.clone()),
            buffer_compressed: false,
            local_os: None,
            connection_info: None,
            terminal_type: None,
        }),
        turn,
        transcript_ref: message.transcript_ref.clone(),
        summary_ref: message.summary_ref.clone(),
        model: message.model.clone(),
        branches: message.branches.clone(),
    }
}

fn role_to_str(role: AiChatRole) -> &'static str {
    match role {
        AiChatRole::User => "user",
        AiChatRole::Assistant => "assistant",
        AiChatRole::System => "system",
        AiChatRole::Tool => "tool",
    }
}

fn role_from_str(role: &str) -> AiChatRole {
    match role {
        "assistant" => AiChatRole::Assistant,
        "system" => AiChatRole::System,
        "tool" => AiChatRole::Tool,
        _ => AiChatRole::User,
    }
}

fn encode_anchor_content(content: &str, metadata: &AiChatMessageMetadata) -> String {
    let metadata_json = serde_json::to_vec(metadata).unwrap_or_default();
    let b64 = general_purpose::STANDARD.encode(metadata_json);
    format!("{ANCHOR_META_HEADER}{b64}\n{content}")
}

fn decode_anchor_content(content: &str) -> Option<(String, Option<AiChatMessageMetadata>)> {
    let rest = content.strip_prefix(ANCHOR_META_HEADER)?;
    let (b64, real_content) = rest.split_once('\n')?;
    let bytes = general_purpose::STANDARD.decode(b64).ok()?;
    let metadata = serde_json::from_slice::<AiChatMessageMetadata>(&bytes).ok()?;
    Some((real_content.to_string(), Some(metadata)))
}

fn legacy_turn_from_message(message: &AiChatMessage) -> Value {
    let mut parts = Vec::new();
    if let Some(thinking) = message
        .thinking_content
        .as_ref()
        .filter(|value| !value.is_empty())
    {
        parts.push(serde_json::json!({ "type": "thinking", "text": thinking }));
    }
    if !message.content.is_empty() {
        parts.push(serde_json::json!({ "type": "text", "text": message.content }));
    }
    serde_json::json!({
        "id": message.id,
        "status": "complete",
        "parts": parts,
        "toolRounds": [],
        "plainTextSummary": message.content,
    })
}

fn thinking_content_from_turn(turn: &Value) -> Option<String> {
    let parts = turn.get("parts")?.as_array()?;
    let content = parts
        .iter()
        .filter(|part| part.get("type").and_then(Value::as_str) == Some("thinking"))
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("");
    (!content.is_empty()).then_some(content)
}

fn parse_thinking_tags(content: &str) -> Option<(String, String)> {
    let mut thinking = Vec::new();
    let mut output = String::new();
    let mut rest = content;
    loop {
        let Some(start) = rest.find("<thinking>") else {
            output.push_str(rest);
            break;
        };
        output.push_str(&rest[..start]);
        let after_start = &rest[start + "<thinking>".len()..];
        let Some(end) = after_start.find("</thinking>") else {
            output.push_str(&rest[start..]);
            break;
        };
        let thought = after_start[..end].trim();
        if !thought.is_empty() {
            thinking.push(thought.to_string());
        }
        rest = &after_start[end + "</thinking>".len()..];
    }
    (!thinking.is_empty()).then_some((output.trim().to_string(), thinking.join("\n\n")))
}

fn compress_buffer(content: &str) -> (String, bool) {
    if content.len() < COMPRESSION_THRESHOLD {
        return (content.to_string(), false);
    }
    match zstd::encode_all(content.as_bytes(), 3) {
        Ok(compressed) if compressed.len() < content.len() => {
            (general_purpose::STANDARD.encode(compressed), true)
        }
        _ => (content.to_string(), false),
    }
}

fn decompress_buffer(content: &str, compressed: bool) -> Result<String> {
    if !compressed {
        return Ok(content.to_string());
    }
    let bytes = general_purpose::STANDARD
        .decode(content)
        .context("failed to decode compressed AI context buffer")?;
    let decompressed =
        zstd::decode_all(bytes.as_slice()).context("failed to decompress AI context buffer")?;
    String::from_utf8(decompressed).context("AI context buffer was not valid UTF-8")
}

fn effective_projection_updated_at(message: &PersistedMessage) -> i64 {
    if message.projection_updated_at > 0 {
        message.projection_updated_at
    } else {
        message.timestamp
    }
}

fn should_replace_projection(incoming: &PersistedMessage, existing: &PersistedMessage) -> bool {
    match (
        incoming.projection_updated_at > 0,
        existing.projection_updated_at > 0,
    ) {
        (false, true) => false,
        (true, false) => incoming.projection_updated_at >= existing.timestamp,
        _ => effective_projection_updated_at(incoming) >= effective_projection_updated_at(existing),
    }
}

fn default_origin() -> String {
    "sidebar".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSnapshot {
    pub cwd: Option<String>,
    pub selection: Option<String>,
    pub buffer_tail: Option<String>,
    #[serde(default)]
    pub buffer_compressed: bool,
    pub local_os: Option<String>,
    pub connection_info: Option<String>,
    pub terminal_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedToolResult {
    pub tool_call_id: String,
    pub tool_name: String,
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
    pub truncated: Option<bool>,
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersistedToolCallStatus {
    Pending,
    Approved,
    Rejected,
    Running,
    Completed,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
    pub status: PersistedToolCallStatus,
    pub result: Option<PersistedToolResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedMessage {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub timestamp: i64,
    #[serde(default)]
    pub projection_updated_at: i64,
    #[serde(default)]
    pub tool_calls: Vec<PersistedToolCall>,
    #[serde(default)]
    pub tool_call_id: Option<String>,
    pub context_snapshot: Option<ContextSnapshot>,
    #[serde(default)]
    pub turn: Option<Value>,
    #[serde(default)]
    pub transcript_ref: Option<Value>,
    #[serde(default)]
    pub summary_ref: Option<Value>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub branches: Option<AiMessageBranches>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMeta {
    pub id: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub message_count: usize,
    pub session_id: Option<String>,
    #[serde(default = "default_origin")]
    pub origin: String,
    #[serde(default)]
    pub session_metadata: Option<Value>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
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
