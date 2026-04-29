// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! AI Chat persistence using redb
//!
//! Provides persistent storage for AI conversations and messages with:
//! - Conversation metadata (id, title, timestamps)
//! - Message snapshots with context (cwd, selection, bufferTail)
//! - Optional zstd compression for buffer snapshots
//!
//! Database: chat_history.redb
//! Tables:
//!   - conversations: UUID -> ConversationMeta (JSON)
//!   - messages: MessageID -> MessageSnapshot (JSON, optionally compressed)

#![allow(clippy::result_large_err)]

use redb::{Database, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[cfg(test)]
use std::cell::Cell;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, error, info, warn};

// ═══════════════════════════════════════════════════════════════════════════
// Constants
// ═══════════════════════════════════════════════════════════════════════════

/// Database version for migrations
pub const AI_CHAT_DB_VERSION: u32 = 3;

/// Maximum conversations to keep (LRU eviction)
pub const MAX_CONVERSATIONS: usize = 100;

/// Maximum messages per conversation
pub const MAX_MESSAGES_PER_CONVERSATION: usize = 200;

/// Compression threshold (compress if buffer > 4KB)
const COMPRESSION_THRESHOLD: usize = 4096;

#[cfg(test)]
std::thread_local! {
    static TEST_FORCE_COMPRESSION_FAILURE: Cell<bool> = const { Cell::new(false) };
}

// ═══════════════════════════════════════════════════════════════════════════
// Table Definitions
// ═══════════════════════════════════════════════════════════════════════════

/// Table: conversations (key: UUID string, value: MessagePack bytes)
const CONVERSATIONS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("conversations");

/// Table: messages (key: message_id string, value: MessagePack bytes)
const MESSAGES_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("messages");

/// Table: conversation_messages (key: conv_id, value: Vec<message_id> as MessagePack)
const CONV_MESSAGES_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("conversation_messages");

/// Table: transcript entries (key: entry_id, value: MessagePack bytes)
const TRANSCRIPT_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("ai_chat_transcript");

/// Table: conversation transcript index (key: conv_id, value: Vec<entry_id> as MessagePack)
const CONV_TRANSCRIPT_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("conversation_transcript");

/// Table: metadata (key: string, value: MessagePack bytes)
const METADATA_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("ai_chat_metadata");

/// Table: diagnostic events (key: event_id, value: MessagePack bytes)
const DIAGNOSTIC_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("ai_chat_diagnostic_events");

/// Table: conversation diagnostic index (key: conv_id, value: Vec<event_id> as MessagePack)
const CONV_DIAGNOSTIC_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("conversation_diagnostic_events");

// ═══════════════════════════════════════════════════════════════════════════
// Data Types
// ═══════════════════════════════════════════════════════════════════════════

/// Context snapshot captured at message send time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSnapshot {
    /// Current working directory (if available)
    pub cwd: Option<String>,
    /// Selected text in terminal (if any)
    pub selection: Option<String>,
    /// Last N lines of terminal buffer (may be compressed)
    pub buffer_tail: Option<String>,
    /// Whether buffer_tail is zstd compressed
    #[serde(default)]
    pub buffer_compressed: bool,
    /// Local OS at the time of message
    pub local_os: Option<String>,
    /// SSH connection info (user@host)
    pub connection_info: Option<String>,
    /// Terminal type (ssh/local)
    pub terminal_type: Option<String>,
}

/// Result of a tool execution persisted with an assistant message
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

/// Valid tool execution states persisted with assistant messages
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

/// Tool call metadata persisted with an assistant message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
    pub status: PersistedToolCallStatus,
    pub result: Option<PersistedToolResult>,
}

/// A single message in an AI conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedMessage {
    /// Unique message ID
    pub id: String,
    /// Parent conversation ID
    pub conversation_id: String,
    /// Message role
    pub role: String, // "user" | "assistant" | "system"
    /// Message content
    pub content: String,
    /// Unix timestamp (ms)
    pub timestamp: i64,
    /// Monotonic projection write marker used to reject stale async overwrites.
    #[serde(default)]
    pub projection_updated_at: i64,
    /// Tool calls associated with this assistant message
    #[serde(default)]
    pub tool_calls: Vec<PersistedToolCall>,
    /// Context snapshot at send time
    pub context_snapshot: Option<ContextSnapshot>,
    /// Structured assistant turn payload (optional during migration)
    #[serde(default)]
    pub turn: Option<Value>,
    /// Transcript reference for the assistant output (optional during migration)
    #[serde(default)]
    pub transcript_ref: Option<Value>,
    /// Summary reference metadata for reload/rebuild (optional during migration)
    #[serde(default)]
    pub summary_ref: Option<Value>,
    /// Model that produced this assistant message. Keep this as the final
    /// field so old MessagePack records remain position-compatible.
    #[serde(default)]
    pub model: Option<String>,
}

/// Conversation metadata (lightweight, for list display)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMeta {
    /// Unique conversation ID
    pub id: String,
    /// Conversation title
    pub title: String,
    /// Creation timestamp (ms)
    pub created_at: i64,
    /// Last update timestamp (ms)
    pub updated_at: i64,
    /// Message count (cached for display)
    pub message_count: usize,
    /// Associated session ID (optional)
    pub session_id: Option<String>,
    /// Origin: "sidebar", "inline", "cli" (defaults to "sidebar" for existing data)
    #[serde(default = "default_origin")]
    pub origin: String,
    /// Session metadata for turn-first conversations (optional during migration)
    #[serde(default)]
    pub session_metadata: Option<Value>,
}

fn default_origin() -> String {
    "sidebar".to_string()
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

/// Full conversation with messages (for loading)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullConversation {
    pub meta: ConversationMeta,
    pub messages: Vec<PersistedMessage>,
}

/// Transcript entry persisted for append-only assistant/user/tool facts.
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

/// Diagnostic event persisted separately from transcript/projection.
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

// ═══════════════════════════════════════════════════════════════════════════
// Error Types
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Error)]
#[allow(clippy::result_large_err)]
pub enum AiChatError {
    #[error("Database error: {0}")]
    Database(#[from] redb::DatabaseError),

    #[error("Transaction error: {0}")]
    Transaction(#[from] redb::TransactionError),

    #[error("Table error: {0}")]
    Table(#[from] redb::TableError),

    #[error("Storage error: {0}")]
    Storage(#[from] redb::StorageError),

    #[error("Commit error: {0}")]
    Commit(#[from] redb::CommitError),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Compression error: {0}")]
    Compression(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Conversation not found: {0}")]
    NotFound(String),

    #[error("Conversation changed during compaction: {0}")]
    Conflict(String),
}

impl From<rmp_serde::encode::Error> for AiChatError {
    fn from(e: rmp_serde::encode::Error) -> Self {
        AiChatError::Serialization(e.to_string())
    }
}

impl From<rmp_serde::decode::Error> for AiChatError {
    fn from(e: rmp_serde::decode::Error) -> Self {
        AiChatError::Serialization(e.to_string())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Compression Utilities
// ═══════════════════════════════════════════════════════════════════════════

/// Compress buffer content using zstd if above threshold
fn compress_buffer(content: &str) -> (String, bool) {
    if content.len() < COMPRESSION_THRESHOLD {
        return (content.to_string(), false);
    }

    #[cfg(test)]
    if TEST_FORCE_COMPRESSION_FAILURE.with(|flag| flag.get()) {
        return (content.to_string(), false);
    }

    // Use zstd compression level 3 (fast, reasonable ratio)
    match zstd::encode_all(content.as_bytes(), 3) {
        Ok(compressed) => {
            // Only use compression if it actually reduces size
            if compressed.len() < content.len() {
                let encoded =
                    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &compressed);
                debug!(
                    "Compressed buffer: {} -> {} bytes ({:.1}% reduction)",
                    content.len(),
                    encoded.len(),
                    (1.0 - (encoded.len() as f64 / content.len() as f64)) * 100.0
                );
                (encoded, true)
            } else {
                (content.to_string(), false)
            }
        }
        Err(e) => {
            warn!("Failed to compress buffer: {}", e);
            (content.to_string(), false)
        }
    }
}

/// Decompress buffer content if compressed
fn decompress_buffer(content: &str, is_compressed: bool) -> Result<String, AiChatError> {
    if !is_compressed {
        return Ok(content.to_string());
    }

    let compressed = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, content)
        .map_err(|e| AiChatError::Compression(format!("Base64 decode failed: {}", e)))?;

    let decompressed = zstd::decode_all(compressed.as_slice())
        .map_err(|e| AiChatError::Compression(format!("Zstd decompress failed: {}", e)))?;

    String::from_utf8(decompressed)
        .map_err(|e| AiChatError::Compression(format!("UTF-8 decode failed: {}", e)))
}

#[cfg(test)]
fn set_test_force_compression_failure(enabled: bool) {
    TEST_FORCE_COMPRESSION_FAILURE.with(|flag| flag.set(enabled));
}

// ═══════════════════════════════════════════════════════════════════════════
// AI Chat Store
// ═══════════════════════════════════════════════════════════════════════════

/// AI Chat persistence store using redb
pub struct AiChatStore {
    db: Arc<Database>,
    path: PathBuf,
}

impl AiChatStore {
    /// Create a new AI chat store at the given path
    pub fn new(path: PathBuf) -> Result<Self, AiChatError> {
        let db = match Database::create(&path) {
            Ok(db) => {
                info!("AI chat database opened at {:?}", path);
                db
            }
            Err(redb::DatabaseError::DatabaseAlreadyOpen) => {
                error!("AI chat database is locked by another process: {:?}", path);
                return Err(redb::DatabaseError::DatabaseAlreadyOpen.into());
            }
            Err(redb::DatabaseError::UpgradeRequired(v)) => {
                error!(
                    "AI chat database at {:?} requires format upgrade (version {})",
                    path, v
                );
                return Err(redb::DatabaseError::UpgradeRequired(v).into());
            }
            Err(redb::DatabaseError::Storage(redb::StorageError::Corrupted(message))) => {
                warn!(
                    "AI chat database at {:?} is corrupted ({}), attempting recovery",
                    path, message
                );

                // Backup corrupted file
                let backup_path = path.with_extension("redb.backup");
                if let Err(e) = std::fs::rename(&path, &backup_path) {
                    error!("Failed to backup corrupted AI chat database: {:?}", e);
                } else {
                    info!("Backed up corrupted AI chat database to {:?}", backup_path);
                }

                Database::create(&path)?
            }
            Err(e) => {
                error!("Failed to open AI chat database at {:?}: {:?}", path, e);
                return Err(e.into());
            }
        };

        // Set file permissions to 600 (owner read/write only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Err(e) = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            {
                warn!("Failed to set AI chat database permissions: {}", e);
            }
        }

        let store = Self {
            db: Arc::new(db),
            path,
        };
        store.initialize()?;

        Ok(store)
    }

    /// Initialize database tables
    fn initialize(&self) -> Result<(), AiChatError> {
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

        // Check/set version
        self.check_version()?;

        info!("AI chat store initialized");
        Ok(())
    }

    /// Check and set database version
    fn check_version(&self) -> Result<(), AiChatError> {
        let write_txn = self.db.begin_write()?;

        {
            let mut table = write_txn.open_table(METADATA_TABLE)?;

            // Read existing version first
            let existing_version = table
                .get("version")?
                .map(|v| rmp_serde::from_slice::<u32>(v.value()).ok())
                .flatten();

            match existing_version {
                Some(version) if version < AI_CHAT_DB_VERSION => {
                    info!(
                        "Migrating AI chat database from v{} to v{}",
                        version, AI_CHAT_DB_VERSION
                    );
                    // Add migration logic here if needed
                    let version_bytes = rmp_serde::to_vec(&AI_CHAT_DB_VERSION)?;
                    table.insert("version", version_bytes.as_slice())?;
                }
                None => {
                    let version_bytes = rmp_serde::to_vec(&AI_CHAT_DB_VERSION)?;
                    table.insert("version", version_bytes.as_slice())?;
                }
                _ => {} // Version is current, no action needed
            }
        }

        write_txn.commit()?;
        Ok(())
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Conversation Operations
    // ═══════════════════════════════════════════════════════════════════════

    /// Create a new conversation
    pub fn create_conversation(&self, meta: &ConversationMeta) -> Result<(), AiChatError> {
        let write_txn = self.db.begin_write()?;

        {
            let mut conv_table = write_txn.open_table(CONVERSATIONS_TABLE)?;
            let mut msg_index_table = write_txn.open_table(CONV_MESSAGES_TABLE)?;

            // Serialize and store conversation meta
            let meta_bytes = rmp_serde::to_vec(meta)?;
            conv_table.insert(meta.id.as_str(), meta_bytes.as_slice())?;

            // Initialize empty message list
            let empty_list: Vec<String> = vec![];
            let list_bytes = rmp_serde::to_vec(&empty_list)?;
            msg_index_table.insert(meta.id.as_str(), list_bytes.as_slice())?;

            let mut transcript_index_table = write_txn.open_table(CONV_TRANSCRIPT_TABLE)?;
            transcript_index_table.insert(meta.id.as_str(), list_bytes.as_slice())?;

            let mut diagnostic_index_table = write_txn.open_table(CONV_DIAGNOSTIC_TABLE)?;
            diagnostic_index_table.insert(meta.id.as_str(), list_bytes.as_slice())?;
        }

        write_txn.commit()?;

        // Enforce conversation limit (LRU eviction)
        self.enforce_conversation_limit()?;

        debug!("Created conversation: {}", meta.id);
        Ok(())
    }

    /// Update conversation metadata
    pub fn update_conversation(&self, meta: &ConversationMeta) -> Result<(), AiChatError> {
        let write_txn = self.db.begin_write()?;

        {
            let mut conv_table = write_txn.open_table(CONVERSATIONS_TABLE)?;
            let meta_bytes = rmp_serde::to_vec(meta)?;
            conv_table.insert(meta.id.as_str(), meta_bytes.as_slice())?;
        }

        write_txn.commit()?;
        debug!("Updated conversation: {}", meta.id);
        Ok(())
    }

    /// Delete a conversation and all its messages
    pub fn delete_conversation(&self, conversation_id: &str) -> Result<(), AiChatError> {
        let write_txn = self.db.begin_write()?;

        {
            let mut conv_table = write_txn.open_table(CONVERSATIONS_TABLE)?;
            let mut msg_table = write_txn.open_table(MESSAGES_TABLE)?;
            let mut msg_index_table = write_txn.open_table(CONV_MESSAGES_TABLE)?;
            let mut transcript_table = write_txn.open_table(TRANSCRIPT_TABLE)?;
            let mut transcript_index_table = write_txn.open_table(CONV_TRANSCRIPT_TABLE)?;
            let mut diagnostic_table = write_txn.open_table(DIAGNOSTIC_TABLE)?;
            let mut diagnostic_index_table = write_txn.open_table(CONV_DIAGNOSTIC_TABLE)?;

            // Get message IDs for this conversation
            if let Some(list_bytes) = msg_index_table.get(conversation_id)? {
                let message_ids: Vec<String> = rmp_serde::from_slice(list_bytes.value())?;

                // Delete all messages
                for msg_id in message_ids {
                    let _ = msg_table.remove(msg_id.as_str());
                }
            }

            // Delete message index
            let _ = msg_index_table.remove(conversation_id);

            if let Some(list_bytes) = transcript_index_table.get(conversation_id)? {
                let transcript_ids: Vec<String> = rmp_serde::from_slice(list_bytes.value())?;
                for entry_id in transcript_ids {
                    let _ = transcript_table.remove(entry_id.as_str());
                }
            }

            let _ = transcript_index_table.remove(conversation_id);

            if let Some(list_bytes) = diagnostic_index_table.get(conversation_id)? {
                let diagnostic_ids: Vec<String> = rmp_serde::from_slice(list_bytes.value())?;
                for event_id in diagnostic_ids {
                    let _ = diagnostic_table.remove(event_id.as_str());
                }
            }

            let _ = diagnostic_index_table.remove(conversation_id);

            // Delete conversation meta
            let _ = conv_table.remove(conversation_id);
        }

        write_txn.commit()?;
        info!("Deleted conversation: {}", conversation_id);
        Ok(())
    }

    /// List all conversations (metadata only, sorted by updated_at desc)
    pub fn list_conversations(&self) -> Result<Vec<ConversationMeta>, AiChatError> {
        let read_txn = self.db.begin_read()?;
        let conv_table = read_txn.open_table(CONVERSATIONS_TABLE)?;

        let mut conversations: Vec<ConversationMeta> = Vec::new();
        let mut total_rows = 0usize;
        let mut failed_rows = 0usize;

        for result in conv_table.iter()? {
            total_rows += 1;
            let (key, value) = result?;
            match rmp_serde::from_slice::<ConversationMeta>(value.value()) {
                Ok(meta) => conversations.push(meta),
                Err(e) => {
                    failed_rows += 1;
                    warn!(
                        "Failed to deserialize conversation '{}': {}",
                        key.value(),
                        e
                    );
                }
            }
        }

        if failed_rows > 0 {
            warn!(
                "list_conversations: {}/{} rows failed to deserialize",
                failed_rows, total_rows
            );
        }

        if total_rows > 0 && failed_rows == total_rows {
            return Err(AiChatError::Serialization(
                "all conversations failed to deserialize".to_string(),
            ));
        }

        debug!(
            "list_conversations: returning {} conversations (total_rows={}, failed={})",
            conversations.len(),
            total_rows,
            failed_rows
        );

        // Sort by updated_at descending (newest first)
        conversations.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        Ok(conversations)
    }

    /// Get a single conversation with all messages
    pub fn get_conversation(&self, conversation_id: &str) -> Result<FullConversation, AiChatError> {
        let read_txn = self.db.begin_read()?;

        // Get conversation meta
        let conv_table = read_txn.open_table(CONVERSATIONS_TABLE)?;
        let meta_bytes = conv_table
            .get(conversation_id)?
            .ok_or_else(|| AiChatError::NotFound(conversation_id.to_string()))?;
        let meta: ConversationMeta = rmp_serde::from_slice(meta_bytes.value())?;

        // Get message IDs
        let msg_index_table = read_txn.open_table(CONV_MESSAGES_TABLE)?;
        let message_ids: Vec<String> =
            if let Some(list_bytes) = msg_index_table.get(conversation_id)? {
                rmp_serde::from_slice(list_bytes.value())?
            } else {
                vec![]
            };

        // Load messages
        let msg_table = read_txn.open_table(MESSAGES_TABLE)?;
        let mut messages: Vec<PersistedMessage> = Vec::new();

        for msg_id in message_ids {
            if let Some(msg_bytes) = msg_table.get(msg_id.as_str())? {
                let mut msg: PersistedMessage = rmp_serde::from_slice(msg_bytes.value())?;

                // Decompress buffer if needed
                if let Some(ref mut ctx) = msg.context_snapshot {
                    if let Some(ref buffer) = ctx.buffer_tail {
                        ctx.buffer_tail = Some(decompress_buffer(buffer, ctx.buffer_compressed)?);
                        ctx.buffer_compressed = false;
                    }
                }

                messages.push(msg);
            }
        }

        // Sort messages by timestamp
        messages.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

        Ok(FullConversation { meta, messages })
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Message Operations
    // ═══════════════════════════════════════════════════════════════════════

    /// Save a message to a conversation
    pub fn save_message(&self, mut message: PersistedMessage) -> Result<(), AiChatError> {
        // Compress buffer if present and above threshold
        if let Some(ref mut ctx) = message.context_snapshot {
            if let Some(ref buffer) = ctx.buffer_tail {
                let (compressed, is_compressed) = compress_buffer(buffer);
                ctx.buffer_tail = Some(compressed);
                ctx.buffer_compressed = is_compressed;
            }
        }

        let write_txn = self.db.begin_write()?;

        {
            let mut msg_table = write_txn.open_table(MESSAGES_TABLE)?;
            let mut msg_index_table = write_txn.open_table(CONV_MESSAGES_TABLE)?;
            let mut conv_table = write_txn.open_table(CONVERSATIONS_TABLE)?;

            let mut meta: ConversationMeta = conv_table
                .get(message.conversation_id.as_str())?
                .map(|meta_bytes| rmp_serde::from_slice(meta_bytes.value()).ok())
                .flatten()
                .ok_or_else(|| AiChatError::NotFound(message.conversation_id.clone()))?;

            let should_write_message = msg_table
                .get(message.id.as_str())?
                .map(|existing_bytes| {
                    rmp_serde::from_slice::<PersistedMessage>(existing_bytes.value())
                })
                .transpose()?
                .map(|existing| should_replace_projection(&message, &existing))
                .unwrap_or(true);

            if should_write_message {
                let msg_bytes = rmp_serde::to_vec(&message)?;
                msg_table.insert(message.id.as_str(), msg_bytes.as_slice())?;
            }

            // Update message index - read first, then write
            let existing_ids: Option<Vec<String>> = msg_index_table
                .get(message.conversation_id.as_str())?
                .map(|list_bytes| rmp_serde::from_slice(list_bytes.value()).ok())
                .flatten();

            let mut message_ids = existing_ids.unwrap_or_default();

            if !message_ids.contains(&message.id) {
                message_ids.push(message.id.clone());

                // Enforce message limit per conversation
                while message_ids.len() > MAX_MESSAGES_PER_CONVERSATION {
                    if let Some(old_id) = message_ids.first().cloned() {
                        let _ = msg_table.remove(old_id.as_str());
                        message_ids.remove(0);
                    }
                }

                let list_bytes = rmp_serde::to_vec(&message_ids)?;
                msg_index_table.insert(message.conversation_id.as_str(), list_bytes.as_slice())?;
            }

            meta.updated_at = meta
                .updated_at
                .max(effective_projection_updated_at(&message));
            meta.message_count = message_ids.len();
            let updated_bytes = rmp_serde::to_vec(&meta)?;
            conv_table.insert(message.conversation_id.as_str(), updated_bytes.as_slice())?;
        }

        write_txn.commit()?;
        debug!(
            "Saved message {} to conversation {}",
            message.id, message.conversation_id
        );
        Ok(())
    }

    /// Atomically save a projection message and append transcript entries in a single transaction.
    pub fn save_message_with_transcript_entries(
        &self,
        mut message: PersistedMessage,
        entries: Vec<PersistedTranscriptEntry>,
    ) -> Result<(), AiChatError> {
        if let Some(ref mut ctx) = message.context_snapshot {
            if let Some(ref buffer) = ctx.buffer_tail {
                let (compressed, is_compressed) = compress_buffer(buffer);
                ctx.buffer_tail = Some(compressed);
                ctx.buffer_compressed = is_compressed;
            }
        }

        let write_txn = self.db.begin_write()?;

        {
            let mut msg_table = write_txn.open_table(MESSAGES_TABLE)?;
            let mut msg_index_table = write_txn.open_table(CONV_MESSAGES_TABLE)?;
            let mut conv_table = write_txn.open_table(CONVERSATIONS_TABLE)?;
            let mut transcript_table = write_txn.open_table(TRANSCRIPT_TABLE)?;
            let mut transcript_index_table = write_txn.open_table(CONV_TRANSCRIPT_TABLE)?;

            let mut meta: ConversationMeta = conv_table
                .get(message.conversation_id.as_str())?
                .map(|meta_bytes| rmp_serde::from_slice(meta_bytes.value()).ok())
                .flatten()
                .ok_or_else(|| AiChatError::NotFound(message.conversation_id.clone()))?;

            let should_write_message = msg_table
                .get(message.id.as_str())?
                .map(|existing_bytes| {
                    rmp_serde::from_slice::<PersistedMessage>(existing_bytes.value())
                })
                .transpose()?
                .map(|existing| should_replace_projection(&message, &existing))
                .unwrap_or(true);

            if should_write_message {
                let msg_bytes = rmp_serde::to_vec(&message)?;
                msg_table.insert(message.id.as_str(), msg_bytes.as_slice())?;
            }

            let existing_ids: Option<Vec<String>> = msg_index_table
                .get(message.conversation_id.as_str())?
                .map(|list_bytes| rmp_serde::from_slice(list_bytes.value()).ok())
                .flatten();

            let mut message_ids = existing_ids.unwrap_or_default();

            if !message_ids.contains(&message.id) {
                message_ids.push(message.id.clone());

                while message_ids.len() > MAX_MESSAGES_PER_CONVERSATION {
                    if let Some(old_id) = message_ids.first().cloned() {
                        let _ = msg_table.remove(old_id.as_str());
                        message_ids.remove(0);
                    }
                }

                let list_bytes = rmp_serde::to_vec(&message_ids)?;
                msg_index_table.insert(message.conversation_id.as_str(), list_bytes.as_slice())?;
            }

            let transcript_latest_at = entries
                .iter()
                .map(|entry| entry.timestamp)
                .max()
                .unwrap_or(i64::MIN);

            if !entries.is_empty() {
                let existing_transcript_ids: Option<Vec<String>> = transcript_index_table
                    .get(message.conversation_id.as_str())?
                    .map(|list_bytes| rmp_serde::from_slice(list_bytes.value()).ok())
                    .flatten();

                let mut transcript_ids = existing_transcript_ids.unwrap_or_default();

                for entry in &entries {
                    let entry_bytes = rmp_serde::to_vec(entry)?;
                    transcript_table.insert(entry.id.as_str(), entry_bytes.as_slice())?;
                    if !transcript_ids.contains(&entry.id) {
                        transcript_ids.push(entry.id.clone());
                    }
                }

                let list_bytes = rmp_serde::to_vec(&transcript_ids)?;
                transcript_index_table
                    .insert(message.conversation_id.as_str(), list_bytes.as_slice())?;
            }

            meta.updated_at = meta
                .updated_at
                .max(effective_projection_updated_at(&message))
                .max(transcript_latest_at);
            meta.message_count = message_ids.len();
            let updated_bytes = rmp_serde::to_vec(&meta)?;
            conv_table.insert(message.conversation_id.as_str(), updated_bytes.as_slice())?;
        }

        write_txn.commit()?;
        debug!(
            "Saved message {} with transcript entries to conversation {}",
            message.id, message.conversation_id
        );
        Ok(())
    }

    /// Append transcript entries to a conversation without mutating projection rows.
    pub fn append_transcript_entries(
        &self,
        conversation_id: &str,
        entries: Vec<PersistedTranscriptEntry>,
    ) -> Result<(), AiChatError> {
        if entries.is_empty() {
            return Ok(());
        }

        let entry_count = entries.len();

        let write_txn = self.db.begin_write()?;

        {
            let conv_table = write_txn.open_table(CONVERSATIONS_TABLE)?;
            if conv_table.get(conversation_id)?.is_none() {
                return Err(AiChatError::NotFound(conversation_id.to_string()));
            }

            let mut transcript_table = write_txn.open_table(TRANSCRIPT_TABLE)?;
            let mut transcript_index_table = write_txn.open_table(CONV_TRANSCRIPT_TABLE)?;

            let existing_ids: Option<Vec<String>> = transcript_index_table
                .get(conversation_id)?
                .map(|list_bytes| rmp_serde::from_slice(list_bytes.value()).ok())
                .flatten();

            let mut transcript_ids = existing_ids.unwrap_or_default();

            for entry in entries {
                let entry_bytes = rmp_serde::to_vec(&entry)?;
                transcript_table.insert(entry.id.as_str(), entry_bytes.as_slice())?;
                if !transcript_ids.contains(&entry.id) {
                    transcript_ids.push(entry.id);
                }
            }

            let list_bytes = rmp_serde::to_vec(&transcript_ids)?;
            transcript_index_table.insert(conversation_id, list_bytes.as_slice())?;
        }

        write_txn.commit()?;
        debug!(
            "Appended {} transcript entries to conversation {}",
            entry_count, conversation_id
        );
        Ok(())
    }

    /// Load transcript entries for a conversation in append order.
    pub fn get_transcript_entries(
        &self,
        conversation_id: &str,
    ) -> Result<Vec<PersistedTranscriptEntry>, AiChatError> {
        let read_txn = self.db.begin_read()?;
        let conv_table = read_txn.open_table(CONVERSATIONS_TABLE)?;
        if conv_table.get(conversation_id)?.is_none() {
            return Err(AiChatError::NotFound(conversation_id.to_string()));
        }

        let transcript_index_table = read_txn.open_table(CONV_TRANSCRIPT_TABLE)?;
        let entry_ids: Vec<String> = transcript_index_table
            .get(conversation_id)?
            .map(|list_bytes| rmp_serde::from_slice(list_bytes.value()).ok())
            .flatten()
            .unwrap_or_default();

        let transcript_table = read_txn.open_table(TRANSCRIPT_TABLE)?;
        let mut entries = Vec::new();

        for entry_id in entry_ids {
            if let Some(entry_bytes) = transcript_table.get(entry_id.as_str())? {
                let entry: PersistedTranscriptEntry = rmp_serde::from_slice(entry_bytes.value())?;
                entries.push(entry);
            }
        }

        Ok(entries)
    }

    /// Append diagnostic events for a conversation in stable insertion order.
    pub fn append_diagnostic_events(
        &self,
        conversation_id: &str,
        events: Vec<PersistedDiagnosticEvent>,
    ) -> Result<(), AiChatError> {
        if events.is_empty() {
            return Ok(());
        }

        let event_count = events.len();
        let write_txn = self.db.begin_write()?;

        {
            let conv_table = write_txn.open_table(CONVERSATIONS_TABLE)?;
            if conv_table.get(conversation_id)?.is_none() {
                return Err(AiChatError::NotFound(conversation_id.to_string()));
            }

            let mut diagnostic_table = write_txn.open_table(DIAGNOSTIC_TABLE)?;
            let mut diagnostic_index_table = write_txn.open_table(CONV_DIAGNOSTIC_TABLE)?;

            let existing_ids: Option<Vec<String>> = diagnostic_index_table
                .get(conversation_id)?
                .map(|list_bytes| rmp_serde::from_slice(list_bytes.value()).ok())
                .flatten();

            let mut diagnostic_ids = existing_ids.unwrap_or_default();

            for event in events {
                let event_bytes = rmp_serde::to_vec(&event)?;
                diagnostic_table.insert(event.id.as_str(), event_bytes.as_slice())?;
                if !diagnostic_ids.contains(&event.id) {
                    diagnostic_ids.push(event.id);
                }
            }

            let list_bytes = rmp_serde::to_vec(&diagnostic_ids)?;
            diagnostic_index_table.insert(conversation_id, list_bytes.as_slice())?;
        }

        write_txn.commit()?;
        debug!(
            "Appended {} diagnostic events to conversation {}",
            event_count, conversation_id
        );
        Ok(())
    }

    /// Read the newest diagnostic events for a conversation in chronological order.
    pub fn get_diagnostic_events_tail(
        &self,
        conversation_id: &str,
        count: usize,
    ) -> Result<Vec<PersistedDiagnosticEvent>, AiChatError> {
        let read_txn = self.db.begin_read()?;
        let conv_table = read_txn.open_table(CONVERSATIONS_TABLE)?;
        if conv_table.get(conversation_id)?.is_none() {
            return Err(AiChatError::NotFound(conversation_id.to_string()));
        }

        let diagnostic_index_table = read_txn.open_table(CONV_DIAGNOSTIC_TABLE)?;
        let event_ids: Vec<String> = diagnostic_index_table
            .get(conversation_id)?
            .map(|list_bytes| rmp_serde::from_slice(list_bytes.value()).ok())
            .flatten()
            .unwrap_or_default();

        let tail_start = event_ids.len().saturating_sub(count);
        let diagnostic_table = read_txn.open_table(DIAGNOSTIC_TABLE)?;
        let mut events = Vec::new();

        for event_id in event_ids.into_iter().skip(tail_start) {
            if let Some(event_bytes) = diagnostic_table.get(event_id.as_str())? {
                let event: PersistedDiagnosticEvent = rmp_serde::from_slice(event_bytes.value())?;
                events.push(event);
            }
        }

        Ok(events)
    }

    /// Update a message (for streaming content updates)
    pub fn update_message(&self, message_id: &str, content: &str) -> Result<(), AiChatError> {
        let write_txn = self.db.begin_write()?;

        {
            let mut msg_table = write_txn.open_table(MESSAGES_TABLE)?;

            // Read first, then write
            let existing_msg: Option<PersistedMessage> = msg_table
                .get(message_id)?
                .map(|msg_bytes| rmp_serde::from_slice(msg_bytes.value()).ok())
                .flatten();

            if let Some(mut msg) = existing_msg {
                msg.content = content.to_string();
                let updated_bytes = rmp_serde::to_vec(&msg)?;
                msg_table.insert(message_id, updated_bytes.as_slice())?;
            }
        }

        write_txn.commit()?;
        Ok(())
    }

    /// Atomically replace all messages in a conversation with a single new message.
    ///
    /// This is used by the summarize feature to compress history into one summary
    /// message. The entire operation (delete old messages, update metadata, save new
    /// message) happens inside a single redb write transaction — if any step fails
    /// the whole thing rolls back and the original data is preserved.
    pub fn replace_conversation_messages(
        &self,
        conversation_id: &str,
        title: &str,
        new_message: PersistedMessage,
    ) -> Result<(), AiChatError> {
        let write_txn = self.db.begin_write()?;

        {
            let mut conv_table = write_txn.open_table(CONVERSATIONS_TABLE)?;
            let mut msg_table = write_txn.open_table(MESSAGES_TABLE)?;
            let mut msg_index_table = write_txn.open_table(CONV_MESSAGES_TABLE)?;

            // 1. Delete all existing messages for this conversation
            if let Some(list_bytes) = msg_index_table.get(conversation_id)? {
                let message_ids: Vec<String> = rmp_serde::from_slice(list_bytes.value())?;
                for msg_id in message_ids {
                    // Propagate errors so a delete failure aborts the whole
                    // transaction and preserves the original data.
                    msg_table.remove(msg_id.as_str())?;
                }
            }

            // 2. Save the new summary message
            let msg_bytes = rmp_serde::to_vec(&new_message)?;
            msg_table.insert(new_message.id.as_str(), msg_bytes.as_slice())?;

            // 3. Replace message index with just the new message id
            let new_ids = vec![new_message.id.clone()];
            let list_bytes = rmp_serde::to_vec(&new_ids)?;
            msg_index_table.insert(conversation_id, list_bytes.as_slice())?;

            // 4. Update conversation metadata
            let existing_meta: Option<ConversationMeta> = conv_table
                .get(conversation_id)?
                .map(|meta_bytes| rmp_serde::from_slice(meta_bytes.value()).ok())
                .flatten();

            let now = chrono::Utc::now().timestamp_millis();
            let meta = if let Some(mut m) = existing_meta {
                m.title = title.to_string();
                m.updated_at = now;
                m.message_count = 1;
                m
            } else {
                // Conversation was somehow missing — recreate metadata
                ConversationMeta {
                    id: conversation_id.to_string(),
                    title: title.to_string(),
                    created_at: now,
                    updated_at: now,
                    message_count: 1,
                    session_id: None,
                    origin: default_origin(),
                    session_metadata: None,
                }
            };

            let meta_bytes = rmp_serde::to_vec(&meta)?;
            conv_table.insert(conversation_id, meta_bytes.as_slice())?;
        }

        write_txn.commit()?;
        info!(
            "Atomically replaced conversation {} messages with summary {}",
            conversation_id, new_message.id
        );
        Ok(())
    }

    /// Atomically replace all messages in a conversation with a single new message
    /// and append transcript entries in the same write transaction.
    pub fn replace_conversation_messages_with_transcript_entries(
        &self,
        conversation_id: &str,
        title: &str,
        new_message: PersistedMessage,
        transcript_entries: Vec<PersistedTranscriptEntry>,
    ) -> Result<(), AiChatError> {
        let write_txn = self.db.begin_write()?;

        {
            let mut conv_table = write_txn.open_table(CONVERSATIONS_TABLE)?;
            let mut msg_table = write_txn.open_table(MESSAGES_TABLE)?;
            let mut msg_index_table = write_txn.open_table(CONV_MESSAGES_TABLE)?;
            let mut transcript_table = write_txn.open_table(TRANSCRIPT_TABLE)?;
            let mut transcript_index_table = write_txn.open_table(CONV_TRANSCRIPT_TABLE)?;

            if let Some(list_bytes) = msg_index_table.get(conversation_id)? {
                let message_ids: Vec<String> = rmp_serde::from_slice(list_bytes.value())?;
                for msg_id in message_ids {
                    msg_table.remove(msg_id.as_str())?;
                }
            }

            let msg_bytes = rmp_serde::to_vec(&new_message)?;
            msg_table.insert(new_message.id.as_str(), msg_bytes.as_slice())?;

            let new_ids = vec![new_message.id.clone()];
            let list_bytes = rmp_serde::to_vec(&new_ids)?;
            msg_index_table.insert(conversation_id, list_bytes.as_slice())?;

            if !transcript_entries.is_empty() {
                let existing_transcript_ids: Option<Vec<String>> = transcript_index_table
                    .get(conversation_id)?
                    .map(|list_bytes| rmp_serde::from_slice(list_bytes.value()).ok())
                    .flatten();

                let mut transcript_ids = existing_transcript_ids.unwrap_or_default();

                for entry in transcript_entries {
                    let entry_bytes = rmp_serde::to_vec(&entry)?;
                    transcript_table.insert(entry.id.as_str(), entry_bytes.as_slice())?;
                    if !transcript_ids.contains(&entry.id) {
                        transcript_ids.push(entry.id);
                    }
                }

                let transcript_list_bytes = rmp_serde::to_vec(&transcript_ids)?;
                transcript_index_table.insert(conversation_id, transcript_list_bytes.as_slice())?;
            }

            let existing_meta: Option<ConversationMeta> = conv_table
                .get(conversation_id)?
                .map(|meta_bytes| rmp_serde::from_slice(meta_bytes.value()).ok())
                .flatten();

            let now = chrono::Utc::now().timestamp_millis();
            let meta = if let Some(mut m) = existing_meta {
                m.title = title.to_string();
                m.updated_at = now;
                m.message_count = 1;
                m
            } else {
                ConversationMeta {
                    id: conversation_id.to_string(),
                    title: title.to_string(),
                    created_at: now,
                    updated_at: now,
                    message_count: 1,
                    session_id: None,
                    origin: default_origin(),
                    session_metadata: None,
                }
            };

            let meta_bytes = rmp_serde::to_vec(&meta)?;
            conv_table.insert(conversation_id, meta_bytes.as_slice())?;
        }

        write_txn.commit()?;
        info!(
            "Atomically replaced conversation {} messages with summary {} and appended transcript entries",
            conversation_id, new_message.id
        );
        Ok(())
    }

    /// Atomically replace the full ordered message list, but only if the
    /// current persisted ids still match the expected ids captured by the
    /// caller. This prevents stale compaction writes from clobbering newer
    /// messages appended while compaction was running.
    pub fn replace_conversation_message_list(
        &self,
        conversation_id: &str,
        title: &str,
        expected_message_ids: &[String],
        new_messages: Vec<PersistedMessage>,
    ) -> Result<(), AiChatError> {
        let write_txn = self.db.begin_write()?;

        {
            let mut conv_table = write_txn.open_table(CONVERSATIONS_TABLE)?;
            let mut msg_table = write_txn.open_table(MESSAGES_TABLE)?;
            let mut msg_index_table = write_txn.open_table(CONV_MESSAGES_TABLE)?;

            let current_ids: Vec<String> = msg_index_table
                .get(conversation_id)?
                .map(|list_bytes| rmp_serde::from_slice(list_bytes.value()).ok())
                .flatten()
                .unwrap_or_default();

            if current_ids != expected_message_ids {
                return Err(AiChatError::Conflict(conversation_id.to_string()));
            }

            let current_id_set = current_ids.iter().cloned().collect::<HashSet<_>>();
            let new_ids = new_messages
                .iter()
                .map(|message| message.id.clone())
                .collect::<Vec<_>>();
            let retained_id_set = new_ids.iter().cloned().collect::<HashSet<_>>();

            for msg_id in &current_ids {
                if !retained_id_set.contains(msg_id) {
                    msg_table.remove(msg_id.as_str())?;
                }
            }

            for mut message in new_messages.iter().cloned() {
                if current_id_set.contains(&message.id) {
                    continue;
                }

                if let Some(ref mut ctx) = message.context_snapshot {
                    if let Some(ref buffer) = ctx.buffer_tail {
                        let (compressed, is_compressed) = compress_buffer(buffer);
                        ctx.buffer_tail = Some(compressed);
                        ctx.buffer_compressed = is_compressed;
                    }
                }

                let msg_bytes = rmp_serde::to_vec(&message)?;
                msg_table.insert(message.id.as_str(), msg_bytes.as_slice())?;
            }

            let list_bytes = rmp_serde::to_vec(&new_ids)?;
            msg_index_table.insert(conversation_id, list_bytes.as_slice())?;

            let existing_meta: Option<ConversationMeta> = conv_table
                .get(conversation_id)?
                .map(|meta_bytes| rmp_serde::from_slice(meta_bytes.value()).ok())
                .flatten();

            let now = chrono::Utc::now().timestamp_millis();
            let meta = if let Some(mut m) = existing_meta {
                m.title = title.to_string();
                m.updated_at = now;
                m.message_count = new_messages.len();
                m
            } else {
                ConversationMeta {
                    id: conversation_id.to_string(),
                    title: title.to_string(),
                    created_at: now,
                    updated_at: now,
                    message_count: new_messages.len(),
                    session_id: None,
                    origin: default_origin(),
                    session_metadata: None,
                }
            };

            let meta_bytes = rmp_serde::to_vec(&meta)?;
            conv_table.insert(conversation_id, meta_bytes.as_slice())?;
        }

        write_txn.commit()?;
        info!(
            "Atomically replaced conversation {} with {} ordered messages",
            conversation_id,
            new_messages.len()
        );
        Ok(())
    }

    /// Atomically replace the full ordered message list and append transcript entries
    /// in the same write transaction.
    pub fn replace_conversation_message_list_with_transcript_entries(
        &self,
        conversation_id: &str,
        title: &str,
        expected_message_ids: &[String],
        new_messages: Vec<PersistedMessage>,
        transcript_entries: Vec<PersistedTranscriptEntry>,
    ) -> Result<(), AiChatError> {
        let write_txn = self.db.begin_write()?;

        {
            let mut conv_table = write_txn.open_table(CONVERSATIONS_TABLE)?;
            let mut msg_table = write_txn.open_table(MESSAGES_TABLE)?;
            let mut msg_index_table = write_txn.open_table(CONV_MESSAGES_TABLE)?;
            let mut transcript_table = write_txn.open_table(TRANSCRIPT_TABLE)?;
            let mut transcript_index_table = write_txn.open_table(CONV_TRANSCRIPT_TABLE)?;

            let current_ids: Vec<String> = msg_index_table
                .get(conversation_id)?
                .map(|list_bytes| rmp_serde::from_slice(list_bytes.value()).ok())
                .flatten()
                .unwrap_or_default();

            if current_ids != expected_message_ids {
                return Err(AiChatError::Conflict(conversation_id.to_string()));
            }

            let current_id_set = current_ids.iter().cloned().collect::<HashSet<_>>();
            let new_ids = new_messages
                .iter()
                .map(|message| message.id.clone())
                .collect::<Vec<_>>();
            let retained_id_set = new_ids.iter().cloned().collect::<HashSet<_>>();

            for msg_id in &current_ids {
                if !retained_id_set.contains(msg_id) {
                    msg_table.remove(msg_id.as_str())?;
                }
            }

            for mut message in new_messages.iter().cloned() {
                if current_id_set.contains(&message.id) {
                    continue;
                }

                if let Some(ref mut ctx) = message.context_snapshot {
                    if let Some(ref buffer) = ctx.buffer_tail {
                        let (compressed, is_compressed) = compress_buffer(buffer);
                        ctx.buffer_tail = Some(compressed);
                        ctx.buffer_compressed = is_compressed;
                    }
                }

                let msg_bytes = rmp_serde::to_vec(&message)?;
                msg_table.insert(message.id.as_str(), msg_bytes.as_slice())?;
            }

            let list_bytes = rmp_serde::to_vec(&new_ids)?;
            msg_index_table.insert(conversation_id, list_bytes.as_slice())?;

            if !transcript_entries.is_empty() {
                let existing_transcript_ids: Option<Vec<String>> = transcript_index_table
                    .get(conversation_id)?
                    .map(|list_bytes| rmp_serde::from_slice(list_bytes.value()).ok())
                    .flatten();

                let mut transcript_ids = existing_transcript_ids.unwrap_or_default();

                for entry in transcript_entries {
                    let entry_bytes = rmp_serde::to_vec(&entry)?;
                    transcript_table.insert(entry.id.as_str(), entry_bytes.as_slice())?;
                    if !transcript_ids.contains(&entry.id) {
                        transcript_ids.push(entry.id);
                    }
                }

                let transcript_list_bytes = rmp_serde::to_vec(&transcript_ids)?;
                transcript_index_table.insert(conversation_id, transcript_list_bytes.as_slice())?;
            }

            let existing_meta: Option<ConversationMeta> = conv_table
                .get(conversation_id)?
                .map(|meta_bytes| rmp_serde::from_slice(meta_bytes.value()).ok())
                .flatten();

            let now = chrono::Utc::now().timestamp_millis();
            let meta = if let Some(mut m) = existing_meta {
                m.title = title.to_string();
                m.updated_at = now;
                m.message_count = new_messages.len();
                m
            } else {
                ConversationMeta {
                    id: conversation_id.to_string(),
                    title: title.to_string(),
                    created_at: now,
                    updated_at: now,
                    message_count: new_messages.len(),
                    session_id: None,
                    origin: default_origin(),
                    session_metadata: None,
                }
            };

            let meta_bytes = rmp_serde::to_vec(&meta)?;
            conv_table.insert(conversation_id, meta_bytes.as_slice())?;
        }

        write_txn.commit()?;
        debug!(
            "Atomically replaced conversation {} message list and appended transcript entries",
            conversation_id
        );
        Ok(())
    }

    /// Delete messages after a certain message (for regeneration)
    pub fn delete_messages_after(
        &self,
        conversation_id: &str,
        after_message_id: &str,
    ) -> Result<(), AiChatError> {
        let write_txn = self.db.begin_write()?;

        {
            let mut msg_table = write_txn.open_table(MESSAGES_TABLE)?;
            let mut msg_index_table = write_txn.open_table(CONV_MESSAGES_TABLE)?;
            let mut conv_table = write_txn.open_table(CONVERSATIONS_TABLE)?;

            // Read message list first
            let existing_ids: Option<Vec<String>> = msg_index_table
                .get(conversation_id)?
                .map(|list_bytes| rmp_serde::from_slice(list_bytes.value()).ok())
                .flatten();

            if let Some(mut message_ids) = existing_ids {
                // Find the index of the target message
                if let Some(idx) = message_ids.iter().position(|id| id == after_message_id) {
                    // Delete all messages after this index
                    let to_delete: Vec<String> = message_ids.drain((idx + 1)..).collect();
                    for msg_id in to_delete {
                        let _ = msg_table.remove(msg_id.as_str());
                    }

                    // Update index
                    let list_bytes = rmp_serde::to_vec(&message_ids)?;
                    msg_index_table.insert(conversation_id, list_bytes.as_slice())?;

                    // Read conversation meta first
                    let existing_meta: Option<ConversationMeta> = conv_table
                        .get(conversation_id)?
                        .map(|meta_bytes| rmp_serde::from_slice(meta_bytes.value()).ok())
                        .flatten();

                    // Update conversation message count
                    if let Some(mut meta) = existing_meta {
                        meta.message_count = message_ids.len();
                        let updated_bytes = rmp_serde::to_vec(&meta)?;
                        conv_table.insert(conversation_id, updated_bytes.as_slice())?;
                    }
                }
            }
        }

        write_txn.commit()?;
        Ok(())
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Maintenance
    // ═══════════════════════════════════════════════════════════════════════

    /// Enforce conversation limit by deleting oldest conversations
    fn enforce_conversation_limit(&self) -> Result<(), AiChatError> {
        let conversations = self.list_conversations()?;

        if conversations.len() <= MAX_CONVERSATIONS {
            return Ok(());
        }

        // Delete oldest conversations (already sorted by updated_at desc)
        let to_delete = &conversations[MAX_CONVERSATIONS..];
        for conv in to_delete {
            self.delete_conversation(&conv.id)?;
        }

        info!(
            "Evicted {} old conversations to enforce limit",
            to_delete.len()
        );
        Ok(())
    }

    /// Clear all conversations
    pub fn clear_all(&self) -> Result<(), AiChatError> {
        let write_txn = self.db.begin_write()?;

        {
            // Clear all tables
            let mut conv_table = write_txn.open_table(CONVERSATIONS_TABLE)?;
            let mut msg_table = write_txn.open_table(MESSAGES_TABLE)?;
            let mut msg_index_table = write_txn.open_table(CONV_MESSAGES_TABLE)?;
            let mut transcript_table = write_txn.open_table(TRANSCRIPT_TABLE)?;
            let mut transcript_index_table = write_txn.open_table(CONV_TRANSCRIPT_TABLE)?;
            let mut diagnostic_table = write_txn.open_table(DIAGNOSTIC_TABLE)?;
            let mut diagnostic_index_table = write_txn.open_table(CONV_DIAGNOSTIC_TABLE)?;

            // Collect all keys first (can't delete while iterating)
            let conv_keys: Vec<String> = conv_table
                .iter()?
                .filter_map(|r| r.ok().map(|(k, _)| k.value().to_string()))
                .collect();

            let msg_keys: Vec<String> = msg_table
                .iter()?
                .filter_map(|r| r.ok().map(|(k, _)| k.value().to_string()))
                .collect();

            let idx_keys: Vec<String> = msg_index_table
                .iter()?
                .filter_map(|r| r.ok().map(|(k, _)| k.value().to_string()))
                .collect();

            let transcript_keys: Vec<String> = transcript_table
                .iter()?
                .filter_map(|r| r.ok().map(|(k, _)| k.value().to_string()))
                .collect();

            let transcript_idx_keys: Vec<String> = transcript_index_table
                .iter()?
                .filter_map(|r| r.ok().map(|(k, _)| k.value().to_string()))
                .collect();

            let diagnostic_keys: Vec<String> = diagnostic_table
                .iter()?
                .filter_map(|r| r.ok().map(|(k, _)| k.value().to_string()))
                .collect();

            let diagnostic_idx_keys: Vec<String> = diagnostic_index_table
                .iter()?
                .filter_map(|r| r.ok().map(|(k, _)| k.value().to_string()))
                .collect();

            // Delete all entries
            for key in conv_keys {
                let _ = conv_table.remove(key.as_str());
            }
            for key in msg_keys {
                let _ = msg_table.remove(key.as_str());
            }
            for key in idx_keys {
                let _ = msg_index_table.remove(key.as_str());
            }
            for key in transcript_keys {
                let _ = transcript_table.remove(key.as_str());
            }
            for key in transcript_idx_keys {
                let _ = transcript_index_table.remove(key.as_str());
            }
            for key in diagnostic_keys {
                let _ = diagnostic_table.remove(key.as_str());
            }
            for key in diagnostic_idx_keys {
                let _ = diagnostic_index_table.remove(key.as_str());
            }
        }

        write_txn.commit()?;
        info!("Cleared all AI chat history");
        Ok(())
    }

    /// Get database statistics
    pub fn get_stats(&self) -> Result<AiChatStats, AiChatError> {
        let read_txn = self.db.begin_read()?;

        let conv_table = read_txn.open_table(CONVERSATIONS_TABLE)?;
        let msg_table = read_txn.open_table(MESSAGES_TABLE)?;

        let conversation_count = conv_table.iter()?.count();
        let message_count = msg_table.iter()?.count();

        // Get database file size on disk
        let db_size_bytes = self.path.metadata().map(|m| m.len()).unwrap_or(0);

        Ok(AiChatStats {
            conversation_count,
            message_count,
            db_size_bytes,
        })
    }
}

/// Database statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiChatStats {
    pub conversation_count: usize,
    pub message_count: usize,
    pub db_size_bytes: u64,
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn create_test_store() -> (AiChatStore, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test_ai_chat.redb");
        let store = AiChatStore::new(path).unwrap();
        (store, dir)
    }

    fn create_test_meta(id: &str) -> ConversationMeta {
        ConversationMeta {
            id: id.to_string(),
            title: format!("Conversation {id}"),
            created_at: 1000,
            updated_at: 1000,
            message_count: 0,
            session_id: None,
            origin: "sidebar".to_string(),
            session_metadata: None,
        }
    }

    fn create_test_message(
        conversation_id: &str,
        message_id: &str,
        timestamp: i64,
        content: impl Into<String>,
    ) -> PersistedMessage {
        PersistedMessage {
            id: message_id.to_string(),
            conversation_id: conversation_id.to_string(),
            role: "user".to_string(),
            content: content.into(),
            timestamp,
            projection_updated_at: timestamp,
            tool_calls: vec![],
            context_snapshot: None,
            turn: None,
            transcript_ref: None,
            summary_ref: None,
            model: None,
        }
    }

    #[derive(Serialize)]
    struct LegacyPersistedMessage {
        id: String,
        conversation_id: String,
        role: String,
        content: String,
        timestamp: i64,
        projection_updated_at: i64,
        tool_calls: Vec<PersistedToolCall>,
        context_snapshot: Option<ContextSnapshot>,
        turn: Option<Value>,
        transcript_ref: Option<Value>,
        summary_ref: Option<Value>,
    }

    #[test]
    fn test_persisted_message_deserializes_legacy_records_without_model() {
        let legacy = LegacyPersistedMessage {
            id: "legacy-1".to_string(),
            conversation_id: "conv-legacy".to_string(),
            role: "assistant".to_string(),
            content: "old answer".to_string(),
            timestamp: 1000,
            projection_updated_at: 1100,
            tool_calls: vec![],
            context_snapshot: None,
            turn: None,
            transcript_ref: None,
            summary_ref: None,
        };

        let bytes = rmp_serde::to_vec(&legacy).unwrap();
        let decoded: PersistedMessage = rmp_serde::from_slice(&bytes).unwrap();

        assert_eq!(decoded.id, "legacy-1");
        assert_eq!(decoded.content, "old answer");
        assert_eq!(decoded.projection_updated_at, 1100);
        assert!(decoded.model.is_none());
    }

    #[test]
    fn test_conversation_crud() {
        let (store, _dir) = create_test_store();

        // Create
        let meta = ConversationMeta {
            id: "conv-1".to_string(),
            title: "Test Conversation".to_string(),
            created_at: 1000,
            updated_at: 1000,
            message_count: 0,
            session_id: None,
            origin: "sidebar".to_string(),
            session_metadata: None,
        };
        store.create_conversation(&meta).unwrap();

        // List
        let conversations = store.list_conversations().unwrap();
        assert_eq!(conversations.len(), 1);
        assert_eq!(conversations[0].title, "Test Conversation");

        // Delete
        store.delete_conversation("conv-1").unwrap();
        let conversations = store.list_conversations().unwrap();
        assert_eq!(conversations.len(), 0);
    }

    #[test]
    fn test_message_with_context() {
        let (store, _dir) = create_test_store();

        // Create conversation
        let meta = ConversationMeta {
            id: "conv-1".to_string(),
            title: "Test".to_string(),
            created_at: 1000,
            updated_at: 1000,
            message_count: 0,
            session_id: None,
            origin: "sidebar".to_string(),
            session_metadata: None,
        };
        store.create_conversation(&meta).unwrap();

        // Save message with context
        let message = PersistedMessage {
            id: "msg-1".to_string(),
            conversation_id: "conv-1".to_string(),
            role: "user".to_string(),
            content: "Hello".to_string(),
            timestamp: 1001,
            projection_updated_at: 1001,
            tool_calls: vec![],
            context_snapshot: Some(ContextSnapshot {
                cwd: Some("/home/user".to_string()),
                selection: Some("error: command not found".to_string()),
                buffer_tail: Some("$ ls\nfile1\nfile2\n".to_string()),
                buffer_compressed: false,
                local_os: Some("macOS".to_string()),
                connection_info: Some("user@server.com".to_string()),
                terminal_type: Some("ssh".to_string()),
            }),
            turn: None,
            transcript_ref: None,
            summary_ref: None,
            model: None,
        };
        store.save_message(message).unwrap();

        // Load conversation
        let full = store.get_conversation("conv-1").unwrap();
        assert_eq!(full.messages.len(), 1);
        assert!(full.messages[0].context_snapshot.is_some());

        let ctx = full.messages[0].context_snapshot.as_ref().unwrap();
        assert_eq!(ctx.cwd, Some("/home/user".to_string()));
        assert_eq!(ctx.local_os, Some("macOS".to_string()));
    }

    #[test]
    fn test_buffer_compression() {
        // Test that large buffers get compressed
        let large_buffer = "x".repeat(10000);
        let (compressed, is_compressed) = compress_buffer(&large_buffer);
        assert!(is_compressed);
        assert!(compressed.len() < large_buffer.len());

        // Test decompression
        let decompressed = decompress_buffer(&compressed, true).unwrap();
        assert_eq!(decompressed, large_buffer);

        // Test small buffer doesn't get compressed
        let small_buffer = "small";
        let (result, is_compressed) = compress_buffer(small_buffer);
        assert!(!is_compressed);
        assert_eq!(result, small_buffer);
    }

    #[test]
    fn test_cascading_delete() {
        // Verify that deleting a conversation removes ALL associated data
        let (store, _dir) = create_test_store();

        // Create conversation
        let meta = ConversationMeta {
            id: "conv-cascade".to_string(),
            title: "Cascade Test".to_string(),
            created_at: 1000,
            updated_at: 1000,
            message_count: 0,
            session_id: None,
            origin: "sidebar".to_string(),
            session_metadata: None,
        };
        store.create_conversation(&meta).unwrap();

        // Add multiple messages
        for i in 0..5 {
            let msg = PersistedMessage {
                id: format!("msg-{}", i),
                conversation_id: "conv-cascade".to_string(),
                role: if i % 2 == 0 { "user" } else { "assistant" }.to_string(),
                content: format!("Message {}", i),
                timestamp: 1000 + i as i64,
                projection_updated_at: 1000 + i as i64,
                tool_calls: vec![],
                context_snapshot: None,
                turn: None,
                transcript_ref: None,
                summary_ref: None,
                model: None,
            };
            store.save_message(msg).unwrap();
        }

        // Verify initial state
        let stats_before = store.get_stats().unwrap();
        assert_eq!(stats_before.conversation_count, 1);
        assert_eq!(stats_before.message_count, 5);

        // Delete conversation
        store.delete_conversation("conv-cascade").unwrap();

        // Verify ALL data is cleaned up
        let stats_after = store.get_stats().unwrap();
        assert_eq!(
            stats_after.conversation_count, 0,
            "Conversation meta should be deleted"
        );
        assert_eq!(
            stats_after.message_count, 0,
            "All messages should be deleted"
        );

        // Verify conversation cannot be retrieved
        let result = store.get_conversation("conv-cascade");
        assert!(result.is_err(), "Deleted conversation should not be found");
    }

    #[test]
    fn test_clear_all() {
        let (store, _dir) = create_test_store();

        // Create multiple conversations with messages
        for c in 0..3 {
            let meta = ConversationMeta {
                id: format!("conv-{}", c),
                title: format!("Conversation {}", c),
                created_at: 1000,
                updated_at: 1000,
                message_count: 0,
                session_id: None,
                origin: "sidebar".to_string(),
                session_metadata: None,
            };
            store.create_conversation(&meta).unwrap();

            for m in 0..4 {
                let msg = PersistedMessage {
                    id: format!("conv-{}-msg-{}", c, m),
                    conversation_id: format!("conv-{}", c),
                    role: "user".to_string(),
                    content: "Test".to_string(),
                    timestamp: 1000_i64,
                    projection_updated_at: 1000_i64,
                    tool_calls: vec![],
                    context_snapshot: None,
                    turn: None,
                    transcript_ref: None,
                    summary_ref: None,
                    model: None,
                };
                store.save_message(msg).unwrap();
            }
        }

        // Verify initial state: 3 conversations, 12 messages
        let stats_before = store.get_stats().unwrap();
        assert_eq!(stats_before.conversation_count, 3);
        assert_eq!(stats_before.message_count, 12);

        // Clear all
        store.clear_all().unwrap();

        // Verify complete cleanup
        let stats_after = store.get_stats().unwrap();
        assert_eq!(
            stats_after.conversation_count, 0,
            "All conversations should be cleared"
        );
        assert_eq!(
            stats_after.message_count, 0,
            "All messages should be cleared"
        );
    }

    #[test]
    fn test_compress_buffer_empty_string() {
        let (result, compressed) = compress_buffer("");
        assert!(!compressed);
        assert_eq!(result, "");
    }

    #[test]
    fn test_decompress_not_compressed() {
        let result = decompress_buffer("plain text", false).unwrap();
        assert_eq!(result, "plain text");
    }

    #[test]
    fn test_decompress_invalid_base64() {
        let result = decompress_buffer("!!!not-base64!!!", true);
        assert!(result.is_err());
    }

    #[test]
    fn test_compress_decompress_roundtrip_unicode() {
        let content = "你好世界 ".repeat(2000); // CJK + emoji
        let (compressed, is_compressed) = compress_buffer(&content);
        assert!(is_compressed);

        let decompressed = decompress_buffer(&compressed, true).unwrap();
        assert_eq!(decompressed, content);
    }

    #[test]
    fn test_update_conversation_title() {
        let (store, _dir) = create_test_store();

        let meta = ConversationMeta {
            id: "conv-update".to_string(),
            title: "Original".to_string(),
            created_at: 1000,
            updated_at: 1000,
            message_count: 0,
            session_id: None,
            origin: "sidebar".to_string(),
            session_metadata: None,
        };
        store.create_conversation(&meta).unwrap();

        let updated = ConversationMeta {
            title: "Updated Title".to_string(),
            updated_at: 2000,
            ..meta
        };
        store.update_conversation(&updated).unwrap();

        let conversations = store.list_conversations().unwrap();
        assert_eq!(conversations[0].title, "Updated Title");
    }

    #[test]
    fn test_message_ordering() {
        let (store, _dir) = create_test_store();

        let meta = ConversationMeta {
            id: "conv-order".to_string(),
            title: "Order Test".to_string(),
            created_at: 1000,
            updated_at: 1000,
            message_count: 0,
            session_id: None,
            origin: "sidebar".to_string(),
            session_metadata: None,
        };
        store.create_conversation(&meta).unwrap();

        // Add messages with different timestamps
        for i in (0..5).rev() {
            let msg = PersistedMessage {
                id: format!("msg-{}", i),
                conversation_id: "conv-order".to_string(),
                role: "user".to_string(),
                content: format!("Message {}", i),
                timestamp: 1000 + i as i64,
                projection_updated_at: 1000 + i as i64,
                tool_calls: vec![],
                context_snapshot: None,
                turn: None,
                transcript_ref: None,
                summary_ref: None,
                model: None,
            };
            store.save_message(msg).unwrap();
        }

        let full = store.get_conversation("conv-order").unwrap();
        assert_eq!(full.messages.len(), 5);
        // Messages should be ordered by timestamp
        for i in 0..4 {
            assert!(full.messages[i].timestamp <= full.messages[i + 1].timestamp);
        }
    }

    #[test]
    fn test_update_message_content() {
        let (store, _dir) = create_test_store();

        let meta = ConversationMeta {
            id: "conv-upd-msg".to_string(),
            title: "Test".to_string(),
            created_at: 1000,
            updated_at: 1000,
            message_count: 0,
            session_id: None,
            origin: "sidebar".to_string(),
            session_metadata: None,
        };
        store.create_conversation(&meta).unwrap();

        let msg = PersistedMessage {
            id: "msg-to-update".to_string(),
            conversation_id: "conv-upd-msg".to_string(),
            role: "assistant".to_string(),
            content: "Original response".to_string(),
            timestamp: 1001,
            projection_updated_at: 1001,
            tool_calls: vec![],
            context_snapshot: None,
            turn: None,
            transcript_ref: None,
            summary_ref: None,
            model: None,
        };
        store.save_message(msg).unwrap();

        store
            .update_message("msg-to-update", "Updated response")
            .unwrap();

        let full = store.get_conversation("conv-upd-msg").unwrap();
        assert_eq!(full.messages[0].content, "Updated response");
    }

    #[test]
    fn test_stats() {
        let (store, _dir) = create_test_store();
        let stats = store.get_stats().unwrap();
        assert_eq!(stats.conversation_count, 0);
        assert_eq!(stats.message_count, 0);
    }

    #[test]
    fn test_concurrent_writes_same_conversation_keep_counts_consistent() {
        let (store, _dir) = create_test_store();
        let store = Arc::new(store);
        store
            .create_conversation(&create_test_meta("conv-concurrent"))
            .unwrap();

        std::thread::scope(|scope| {
            for worker in 0..8 {
                let store = Arc::clone(&store);
                scope.spawn(move || {
                    for offset in 0..25 {
                        let index = worker * 25 + offset;
                        store
                            .save_message(create_test_message(
                                "conv-concurrent",
                                &format!("msg-{index}"),
                                1000 + index as i64,
                                format!("message {index}"),
                            ))
                            .unwrap();
                    }
                });
            }
        });

        let full = store.get_conversation("conv-concurrent").unwrap();
        let stats = store.get_stats().unwrap();

        assert_eq!(full.meta.message_count, 200);
        assert_eq!(full.messages.len(), 200);
        assert_eq!(stats.message_count, 200);
        assert!(
            full.messages
                .windows(2)
                .all(|window| window[0].timestamp <= window[1].timestamp)
        );
    }

    #[test]
    fn test_high_frequency_writes_trim_to_max_messages() {
        let (store, _dir) = create_test_store();
        store
            .create_conversation(&create_test_meta("conv-trim"))
            .unwrap();

        for index in 0..(MAX_MESSAGES_PER_CONVERSATION + 25) {
            store
                .save_message(create_test_message(
                    "conv-trim",
                    &format!("msg-{index}"),
                    1000 + index as i64,
                    format!("payload {index}"),
                ))
                .unwrap();
        }

        let full = store.get_conversation("conv-trim").unwrap();
        let stats = store.get_stats().unwrap();

        assert_eq!(full.meta.message_count, MAX_MESSAGES_PER_CONVERSATION);
        assert_eq!(full.messages.len(), MAX_MESSAGES_PER_CONVERSATION);
        assert_eq!(stats.message_count, MAX_MESSAGES_PER_CONVERSATION);
        assert_eq!(full.messages.first().unwrap().id, "msg-25");
        assert_eq!(full.messages.last().unwrap().id, "msg-224");
    }

    #[test]
    fn test_large_message_roundtrip() {
        let (store, _dir) = create_test_store();
        store
            .create_conversation(&create_test_meta("conv-large"))
            .unwrap();

        let large_content = "L".repeat(1_000_000);
        let large_buffer = "buffer-line\n".repeat(2000);
        let message = PersistedMessage {
            id: "msg-large".to_string(),
            conversation_id: "conv-large".to_string(),
            role: "assistant".to_string(),
            content: large_content.clone(),
            timestamp: 5000,
            projection_updated_at: 5000,
            tool_calls: vec![],
            context_snapshot: Some(ContextSnapshot {
                cwd: None,
                selection: None,
                buffer_tail: Some(large_buffer.clone()),
                buffer_compressed: false,
                local_os: None,
                connection_info: None,
                terminal_type: None,
            }),
            turn: None,
            transcript_ref: None,
            summary_ref: None,
            model: None,
        };

        store.save_message(message).unwrap();

        let full = store.get_conversation("conv-large").unwrap();
        assert_eq!(full.messages.len(), 1);
        assert_eq!(full.messages[0].content.len(), large_content.len());
        assert_eq!(full.messages[0].content, large_content);
        assert_eq!(
            full.messages[0]
                .context_snapshot
                .as_ref()
                .and_then(|ctx| ctx.buffer_tail.as_ref())
                .unwrap(),
            &large_buffer
        );
    }

    #[test]
    fn test_save_message_compression_failure_falls_back_to_plaintext() {
        let (store, _dir) = create_test_store();
        store
            .create_conversation(&create_test_meta("conv-fallback"))
            .unwrap();

        let large_buffer = "fallback-data-".repeat(500);
        set_test_force_compression_failure(true);
        let result = store.save_message(PersistedMessage {
            id: "msg-fallback".to_string(),
            conversation_id: "conv-fallback".to_string(),
            role: "user".to_string(),
            content: "trigger fallback".to_string(),
            timestamp: 1001,
            projection_updated_at: 1001,
            tool_calls: vec![],
            context_snapshot: Some(ContextSnapshot {
                cwd: None,
                selection: None,
                buffer_tail: Some(large_buffer.clone()),
                buffer_compressed: false,
                local_os: None,
                connection_info: None,
                terminal_type: None,
            }),
            turn: None,
            transcript_ref: None,
            summary_ref: None,
            model: None,
        });
        set_test_force_compression_failure(false);

        result.unwrap();

        let full = store.get_conversation("conv-fallback").unwrap();
        let ctx = full.messages[0].context_snapshot.as_ref().unwrap();
        assert_eq!(ctx.buffer_tail.as_ref().unwrap(), &large_buffer);
        assert!(!ctx.buffer_compressed);
    }

    #[test]
    fn test_save_message_after_delete_requires_recreate_and_orders_new_messages() {
        let (store, _dir) = create_test_store();
        store
            .create_conversation(&create_test_meta("conv-recreate"))
            .unwrap();
        store
            .save_message(create_test_message("conv-recreate", "old-1", 1001, "old"))
            .unwrap();

        store.delete_conversation("conv-recreate").unwrap();

        let error = store
            .save_message(create_test_message(
                "conv-recreate",
                "orphan",
                1002,
                "orphan",
            ))
            .unwrap_err();
        assert!(matches!(error, AiChatError::NotFound(id) if id == "conv-recreate"));
        assert_eq!(store.get_stats().unwrap().message_count, 0);

        store
            .create_conversation(&create_test_meta("conv-recreate"))
            .unwrap();
        store
            .save_message(create_test_message("conv-recreate", "new-2", 2002, "new 2"))
            .unwrap();
        store
            .save_message(create_test_message("conv-recreate", "new-1", 2001, "new 1"))
            .unwrap();

        let full = store.get_conversation("conv-recreate").unwrap();
        let ids: Vec<&str> = full.messages.iter().map(|msg| msg.id.as_str()).collect();
        assert_eq!(ids, vec!["new-1", "new-2"]);
        assert_eq!(full.meta.message_count, 2);
    }

    #[test]
    fn test_replace_conversation_messages_recreates_missing_metadata_atomically() {
        let (store, _dir) = create_test_store();
        let replacement = PersistedMessage {
            id: "summary-1".to_string(),
            conversation_id: "conv-missing-summary".to_string(),
            role: "assistant".to_string(),
            content: "summary content".to_string(),
            timestamp: 3000,
            projection_updated_at: 3000,
            tool_calls: vec![],
            context_snapshot: None,
            turn: None,
            transcript_ref: None,
            summary_ref: None,
            model: None,
        };

        store
            .replace_conversation_messages("conv-missing-summary", "Recovered Summary", replacement)
            .unwrap();

        let full = store.get_conversation("conv-missing-summary").unwrap();
        assert_eq!(full.meta.title, "Recovered Summary");
        assert_eq!(full.meta.message_count, 1);
        assert_eq!(full.messages.len(), 1);
        assert_eq!(full.messages[0].id, "summary-1");
    }

    #[test]
    fn test_append_and_load_transcript_entries() {
        let (store, _dir) = create_test_store();
        store
            .create_conversation(&create_test_meta("conv-transcript"))
            .unwrap();

        store
            .append_transcript_entries(
                "conv-transcript",
                vec![
                    PersistedTranscriptEntry {
                        id: "entry-1".to_string(),
                        conversation_id: "conv-transcript".to_string(),
                        turn_id: None,
                        parent_id: None,
                        timestamp: 1,
                        kind: "user_message".to_string(),
                        payload: serde_json::json!({ "messageId": "user-1" }),
                    },
                    PersistedTranscriptEntry {
                        id: "entry-2".to_string(),
                        conversation_id: "conv-transcript".to_string(),
                        turn_id: Some("turn-1".to_string()),
                        parent_id: Some("entry-1".to_string()),
                        timestamp: 2,
                        kind: "assistant_turn_start".to_string(),
                        payload: serde_json::json!({ "requestMessageId": "user-1" }),
                    },
                ],
            )
            .unwrap();

        let entries = store.get_transcript_entries("conv-transcript").unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, "entry-1");
        assert_eq!(entries[1].turn_id.as_deref(), Some("turn-1"));
    }

    #[test]
    fn test_append_and_tail_diagnostic_events() {
        let (store, _dir) = create_test_store();
        store
            .create_conversation(&create_test_meta("conv-diagnostics"))
            .unwrap();

        store
            .append_diagnostic_events(
                "conv-diagnostics",
                vec![
                    PersistedDiagnosticEvent {
                        id: "diag-1".to_string(),
                        conversation_id: "conv-diagnostics".to_string(),
                        turn_id: Some("turn-1".to_string()),
                        round_id: None,
                        timestamp: 1,
                        event_type: "llm_request".to_string(),
                        data: serde_json::json!({ "requestKind": "chat" }),
                    },
                    PersistedDiagnosticEvent {
                        id: "diag-2".to_string(),
                        conversation_id: "conv-diagnostics".to_string(),
                        turn_id: Some("turn-1".to_string()),
                        round_id: Some("round-1".to_string()),
                        timestamp: 2,
                        event_type: "assistant_round".to_string(),
                        data: serde_json::json!({ "toolCallCount": 1 }),
                    },
                    PersistedDiagnosticEvent {
                        id: "diag-3".to_string(),
                        conversation_id: "conv-diagnostics".to_string(),
                        turn_id: Some("turn-1".to_string()),
                        round_id: Some("round-1".to_string()),
                        timestamp: 3,
                        event_type: "tool_result".to_string(),
                        data: serde_json::json!({ "success": true }),
                    },
                ],
            )
            .unwrap();

        let tail = store
            .get_diagnostic_events_tail("conv-diagnostics", 2)
            .unwrap();

        assert_eq!(tail.len(), 2);
        assert_eq!(tail[0].id, "diag-2");
        assert_eq!(tail[1].id, "diag-3");
        assert_eq!(tail[0].round_id.as_deref(), Some("round-1"));
    }

    #[test]
    fn test_replace_conversation_messages_with_transcript_entries_is_atomic() {
        let (store, _dir) = create_test_store();
        store
            .create_conversation(&create_test_meta("conv-summary-atomic"))
            .unwrap();
        store
            .save_message(create_test_message(
                "conv-summary-atomic",
                "msg-1",
                1,
                "old",
            ))
            .unwrap();

        let replacement = PersistedMessage {
            id: "summary-atomic".to_string(),
            conversation_id: "conv-summary-atomic".to_string(),
            role: "assistant".to_string(),
            content: "summary".to_string(),
            timestamp: 2,
            projection_updated_at: 2,
            tool_calls: vec![],
            context_snapshot: None,
            turn: None,
            transcript_ref: Some(serde_json::json!({
                "conversationId": "conv-summary-atomic",
                "endEntryId": "entry-summary"
            })),
            model: None,
            summary_ref: Some(serde_json::json!({
                "kind": "conversation",
                "transcriptRef": {
                    "conversationId": "conv-summary-atomic",
                    "endEntryId": "entry-summary"
                }
            })),
        };

        store
            .replace_conversation_messages_with_transcript_entries(
                "conv-summary-atomic",
                "Summary",
                replacement,
                vec![PersistedTranscriptEntry {
                    id: "entry-summary".to_string(),
                    conversation_id: "conv-summary-atomic".to_string(),
                    turn_id: None,
                    parent_id: Some("summary-atomic".to_string()),
                    timestamp: 2,
                    kind: "summary_created".to_string(),
                    payload: serde_json::json!({ "messageId": "summary-atomic", "summaryText": "summary" }),
                }],
            )
            .unwrap();

        let full = store.get_conversation("conv-summary-atomic").unwrap();
        let entries = store.get_transcript_entries("conv-summary-atomic").unwrap();

        assert_eq!(full.messages.len(), 1);
        assert_eq!(full.messages[0].id, "summary-atomic");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "entry-summary");
    }

    #[test]
    fn test_replace_conversation_message_list_with_transcript_entries_is_atomic() {
        let (store, _dir) = create_test_store();
        store
            .create_conversation(&create_test_meta("conv-compact-atomic"))
            .unwrap();
        store
            .save_message(create_test_message(
                "conv-compact-atomic",
                "msg-1",
                1,
                "first",
            ))
            .unwrap();
        store
            .save_message(create_test_message(
                "conv-compact-atomic",
                "msg-2",
                2,
                "second",
            ))
            .unwrap();

        store
            .replace_conversation_message_list_with_transcript_entries(
                "conv-compact-atomic",
                "Compacted",
                &["msg-1".to_string(), "msg-2".to_string()],
                vec![PersistedMessage {
                    id: "anchor-1".to_string(),
                    conversation_id: "conv-compact-atomic".to_string(),
                    role: "system".to_string(),
                    content: "anchor".to_string(),
                    timestamp: 3,
                    projection_updated_at: 3,
                    tool_calls: vec![],
                    context_snapshot: None,
                    turn: None,
                    transcript_ref: Some(serde_json::json!({
                        "conversationId": "conv-compact-atomic",
                        "endEntryId": "entry-anchor"
                    })),
                    summary_ref: Some(serde_json::json!({
                        "kind": "compaction",
                        "transcriptRef": {
                            "conversationId": "conv-compact-atomic",
                            "endEntryId": "entry-anchor"
                        }
                    })),
                    model: None,
                }],
                vec![PersistedTranscriptEntry {
                    id: "entry-anchor".to_string(),
                    conversation_id: "conv-compact-atomic".to_string(),
                    turn_id: None,
                    parent_id: Some("anchor-1".to_string()),
                    timestamp: 3,
                    kind: "summary_created".to_string(),
                    payload: serde_json::json!({ "messageId": "anchor-1", "summaryText": "anchor", "compactedMessageCount": 2 }),
                }],
            )
            .unwrap();

        let full = store.get_conversation("conv-compact-atomic").unwrap();
        let entries = store.get_transcript_entries("conv-compact-atomic").unwrap();

        assert_eq!(full.messages.len(), 1);
        assert_eq!(full.messages[0].id, "anchor-1");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "entry-anchor");
    }

    #[test]
    fn test_save_message_with_transcript_rejects_stale_projection_overwrite() {
        let (store, _dir) = create_test_store();
        store
            .create_conversation(&create_test_meta("conv-stale-projection"))
            .unwrap();

        store
            .save_message_with_transcript_entries(
                PersistedMessage {
                    id: "assistant-1".to_string(),
                    conversation_id: "conv-stale-projection".to_string(),
                    role: "assistant".to_string(),
                    content: "new projection".to_string(),
                    timestamp: 1000,
                    projection_updated_at: 2000,
                    tool_calls: vec![],
                    context_snapshot: None,
                    turn: None,
                    transcript_ref: None,
                    summary_ref: None,
                    model: None,
                },
                vec![],
            )
            .unwrap();

        store
            .save_message_with_transcript_entries(
                PersistedMessage {
                    id: "assistant-1".to_string(),
                    conversation_id: "conv-stale-projection".to_string(),
                    role: "assistant".to_string(),
                    content: "stale projection".to_string(),
                    timestamp: 1000,
                    projection_updated_at: 1500,
                    tool_calls: vec![],
                    context_snapshot: None,
                    turn: None,
                    transcript_ref: None,
                    summary_ref: None,
                    model: None,
                },
                vec![PersistedTranscriptEntry {
                    id: "entry-1".to_string(),
                    conversation_id: "conv-stale-projection".to_string(),
                    turn_id: Some("assistant-1".to_string()),
                    parent_id: None,
                    timestamp: 1100,
                    kind: "assistant_turn_end".to_string(),
                    payload: serde_json::json!({
                        "messageId": "assistant-1",
                        "status": "complete"
                    }),
                }],
            )
            .unwrap();

        let full = store.get_conversation("conv-stale-projection").unwrap();
        assert_eq!(full.messages.len(), 1);
        assert_eq!(full.messages[0].content, "new projection");
        assert_eq!(full.messages[0].projection_updated_at, 2000);

        let transcript = store
            .get_transcript_entries("conv-stale-projection")
            .unwrap();
        assert_eq!(transcript.len(), 1);
        assert_eq!(transcript[0].id, "entry-1");

        let full = store.get_conversation("conv-stale-projection").unwrap();
        assert_eq!(full.meta.updated_at, 2000);
    }

    #[test]
    fn test_save_message_rejects_stale_projection_overwrite() {
        let (store, _dir) = create_test_store();
        store
            .create_conversation(&create_test_meta("conv-stale-save"))
            .unwrap();

        store
            .save_message(PersistedMessage {
                id: "assistant-1".to_string(),
                conversation_id: "conv-stale-save".to_string(),
                role: "assistant".to_string(),
                content: "fresh projection".to_string(),
                timestamp: 1000,
                projection_updated_at: 3000,
                tool_calls: vec![],
                context_snapshot: None,
                turn: None,
                transcript_ref: None,
                summary_ref: None,
                model: None,
            })
            .unwrap();

        store
            .save_message(PersistedMessage {
                id: "assistant-1".to_string(),
                conversation_id: "conv-stale-save".to_string(),
                role: "assistant".to_string(),
                content: "stale projection".to_string(),
                timestamp: 1000,
                projection_updated_at: 2000,
                tool_calls: vec![],
                context_snapshot: None,
                turn: None,
                transcript_ref: None,
                summary_ref: None,
                model: None,
            })
            .unwrap();

        let full = store.get_conversation("conv-stale-save").unwrap();
        assert_eq!(full.messages.len(), 1);
        assert_eq!(full.messages[0].content, "fresh projection");
        assert_eq!(full.messages[0].projection_updated_at, 3000);
        assert_eq!(full.meta.updated_at, 3000);
    }

    #[test]
    fn test_stale_projection_still_appends_transcript_and_advances_meta() {
        let (store, _dir) = create_test_store();
        store
            .create_conversation(&create_test_meta("conv-transcript-only-update"))
            .unwrap();

        store
            .save_message_with_transcript_entries(
                PersistedMessage {
                    id: "assistant-1".to_string(),
                    conversation_id: "conv-transcript-only-update".to_string(),
                    role: "assistant".to_string(),
                    content: "current projection".to_string(),
                    timestamp: 1000,
                    projection_updated_at: 1500,
                    tool_calls: vec![],
                    context_snapshot: None,
                    turn: None,
                    transcript_ref: None,
                    summary_ref: None,
                    model: None,
                },
                vec![],
            )
            .unwrap();

        store
            .save_message_with_transcript_entries(
                PersistedMessage {
                    id: "assistant-1".to_string(),
                    conversation_id: "conv-transcript-only-update".to_string(),
                    role: "assistant".to_string(),
                    content: "older projection".to_string(),
                    timestamp: 1000,
                    projection_updated_at: 1400,
                    tool_calls: vec![],
                    context_snapshot: None,
                    turn: None,
                    transcript_ref: None,
                    summary_ref: None,
                    model: None,
                },
                vec![PersistedTranscriptEntry {
                    id: "entry-newer".to_string(),
                    conversation_id: "conv-transcript-only-update".to_string(),
                    turn_id: Some("assistant-1".to_string()),
                    parent_id: None,
                    timestamp: 2500,
                    kind: "assistant_turn_end".to_string(),
                    payload: serde_json::json!({
                        "messageId": "assistant-1",
                        "status": "complete"
                    }),
                }],
            )
            .unwrap();

        let full = store
            .get_conversation("conv-transcript-only-update")
            .unwrap();
        assert_eq!(full.messages[0].content, "current projection");
        assert_eq!(full.messages[0].projection_updated_at, 1500);
        assert_eq!(full.meta.updated_at, 2500);

        let transcript = store
            .get_transcript_entries("conv-transcript-only-update")
            .unwrap();
        assert_eq!(transcript.len(), 1);
        assert_eq!(transcript[0].id, "entry-newer");
    }

    #[test]
    fn test_delete_conversation_clears_transcript_entries() {
        let (store, _dir) = create_test_store();
        store
            .create_conversation(&create_test_meta("conv-cleanup"))
            .unwrap();
        store
            .append_transcript_entries(
                "conv-cleanup",
                vec![PersistedTranscriptEntry {
                    id: "entry-cleanup".to_string(),
                    conversation_id: "conv-cleanup".to_string(),
                    turn_id: None,
                    parent_id: None,
                    timestamp: 1,
                    kind: "user_message".to_string(),
                    payload: serde_json::json!({}),
                }],
            )
            .unwrap();

        store.delete_conversation("conv-cleanup").unwrap();

        let error = store.get_transcript_entries("conv-cleanup").unwrap_err();
        assert!(matches!(error, AiChatError::NotFound(id) if id == "conv-cleanup"));
    }

    #[test]
    fn test_delete_conversation_clears_diagnostic_events() {
        let (store, _dir) = create_test_store();
        store
            .create_conversation(&create_test_meta("conv-diagnostic-cleanup"))
            .unwrap();
        store
            .append_diagnostic_events(
                "conv-diagnostic-cleanup",
                vec![PersistedDiagnosticEvent {
                    id: "diag-cleanup".to_string(),
                    conversation_id: "conv-diagnostic-cleanup".to_string(),
                    turn_id: Some("turn-1".to_string()),
                    round_id: None,
                    timestamp: 1,
                    event_type: "error".to_string(),
                    data: serde_json::json!({ "message": "boom" }),
                }],
            )
            .unwrap();

        store
            .delete_conversation("conv-diagnostic-cleanup")
            .unwrap();

        let error = store
            .get_diagnostic_events_tail("conv-diagnostic-cleanup", 10)
            .unwrap_err();
        assert!(matches!(error, AiChatError::NotFound(id) if id == "conv-diagnostic-cleanup"));
    }
}
