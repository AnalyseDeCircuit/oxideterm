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
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, error, info, warn};

// ═══════════════════════════════════════════════════════════════════════════
// Constants
// ═══════════════════════════════════════════════════════════════════════════

/// Database version for migrations
pub const AI_CHAT_DB_VERSION: u32 = 1;

/// Maximum conversations to keep (LRU eviction)
pub const MAX_CONVERSATIONS: usize = 100;

/// Maximum messages per conversation
pub const MAX_MESSAGES_PER_CONVERSATION: usize = 200;

/// Compression threshold (compress if buffer > 4KB)
const COMPRESSION_THRESHOLD: usize = 4096;

// ═══════════════════════════════════════════════════════════════════════════
// Table Definitions
// ═══════════════════════════════════════════════════════════════════════════

/// Table: conversations (key: UUID string, value: MessagePack bytes)
const CONVERSATIONS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("conversations");

/// Table: messages (key: message_id string, value: MessagePack bytes)
const MESSAGES_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("messages");

/// Table: conversation_messages (key: conv_id, value: Vec<message_id> as MessagePack)
const CONV_MESSAGES_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("conversation_messages");

/// Table: metadata (key: string, value: MessagePack bytes)
const METADATA_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("ai_chat_metadata");

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
    /// Context snapshot at send time
    pub context_snapshot: Option<ContextSnapshot>,
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
}

/// Full conversation with messages (for loading)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullConversation {
    pub meta: ConversationMeta,
    pub messages: Vec<PersistedMessage>,
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

    // Use zstd compression level 3 (fast, reasonable ratio)
    match zstd::encode_all(content.as_bytes(), 3) {
        Ok(compressed) => {
            // Only use compression if it actually reduces size
            if compressed.len() < content.len() {
                let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &compressed);
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

// ═══════════════════════════════════════════════════════════════════════════
// AI Chat Store
// ═══════════════════════════════════════════════════════════════════════════

/// AI Chat persistence store using redb
pub struct AiChatStore {
    db: Arc<Database>,
}

impl AiChatStore {
    /// Create a new AI chat store at the given path
    pub fn new(path: PathBuf) -> Result<Self, AiChatError> {
        let db = match Database::create(&path) {
            Ok(db) => {
                info!("AI chat database opened at {:?}", path);
                db
            }
            Err(e) => {
                warn!("Failed to open AI chat database: {:?}, attempting recovery", e);

                // Backup corrupted file
                let backup_path = path.with_extension("redb.backup");
                if let Err(e) = std::fs::rename(&path, &backup_path) {
                    error!("Failed to backup corrupted AI chat database: {:?}", e);
                } else {
                    info!("Backed up corrupted AI chat database to {:?}", backup_path);
                }

                Database::create(&path)?
            }
        };

        // Set file permissions to 600 (owner read/write only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Err(e) = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)) {
                warn!("Failed to set AI chat database permissions: {}", e);
            }
        }

        let store = Self { db: Arc::new(db) };
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
            let _ = write_txn.open_table(METADATA_TABLE)?;
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
            let existing_version = table.get("version")?.map(|v| {
                rmp_serde::from_slice::<u32>(v.value()).ok()
            }).flatten();

            match existing_version {
                Some(version) if version < AI_CHAT_DB_VERSION => {
                    info!("Migrating AI chat database from v{} to v{}", version, AI_CHAT_DB_VERSION);
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

        for result in conv_table.iter()? {
            let (_, value) = result?;
            let meta: ConversationMeta = rmp_serde::from_slice(value.value())?;
            conversations.push(meta);
        }

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
        let message_ids: Vec<String> = if let Some(list_bytes) = msg_index_table.get(conversation_id)? {
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

            // Save message
            let msg_bytes = rmp_serde::to_vec(&message)?;
            msg_table.insert(message.id.as_str(), msg_bytes.as_slice())?;

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

            // Update conversation's updated_at and message_count - read first, then write
            let existing_meta: Option<ConversationMeta> = conv_table
                .get(message.conversation_id.as_str())?
                .map(|meta_bytes| rmp_serde::from_slice(meta_bytes.value()).ok())
                .flatten();

            if let Some(mut meta) = existing_meta {
                meta.updated_at = message.timestamp;
                meta.message_count = message_ids.len();
                let updated_bytes = rmp_serde::to_vec(&meta)?;
                conv_table.insert(message.conversation_id.as_str(), updated_bytes.as_slice())?;
            }
        }

        write_txn.commit()?;
        debug!("Saved message {} to conversation {}", message.id, message.conversation_id);
        Ok(())
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

    /// Delete messages after a certain message (for regeneration)
    pub fn delete_messages_after(&self, conversation_id: &str, after_message_id: &str) -> Result<(), AiChatError> {
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

        info!("Evicted {} old conversations to enforce limit", to_delete.len());
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

        Ok(AiChatStats {
            conversation_count,
            message_count,
        })
    }
}

/// Database statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiChatStats {
    pub conversation_count: usize,
    pub message_count: usize,
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn create_test_store() -> (AiChatStore, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test_ai_chat.redb");
        let store = AiChatStore::new(path).unwrap();
        (store, dir)
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
        };
        store.create_conversation(&meta).unwrap();

        // Save message with context
        let message = PersistedMessage {
            id: "msg-1".to_string(),
            conversation_id: "conv-1".to_string(),
            role: "user".to_string(),
            content: "Hello".to_string(),
            timestamp: 1001,
            context_snapshot: Some(ContextSnapshot {
                cwd: Some("/home/user".to_string()),
                selection: Some("error: command not found".to_string()),
                buffer_tail: Some("$ ls\nfile1\nfile2\n".to_string()),
                buffer_compressed: false,
                local_os: Some("macOS".to_string()),
                connection_info: Some("user@server.com".to_string()),
                terminal_type: Some("ssh".to_string()),
            }),
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
}
