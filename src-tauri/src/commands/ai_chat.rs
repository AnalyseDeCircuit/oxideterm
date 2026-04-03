// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! AI Chat Tauri Commands
//!
//! Provides commands for AI conversation persistence:
//! - List conversations (metadata only)
//! - Get full conversation with messages
//! - Save message with context snapshot
//! - Delete/clear conversations

use crate::state::{
    AiChatError, AiChatStore, ContextSnapshot, ConversationMeta, PersistedMessage,
    PersistedToolCall,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

// ═══════════════════════════════════════════════════════════════════════════
// Request/Response Types
// ═══════════════════════════════════════════════════════════════════════════

/// Request to create a new conversation
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateConversationRequest {
    pub id: String,
    pub title: String,
    pub session_id: Option<String>,
    #[serde(default = "default_origin")]
    pub origin: String,
}

fn default_origin() -> String {
    "sidebar".to_string()
}

/// Request to save a message
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveMessageRequest {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub timestamp: i64,
    #[serde(default)]
    pub tool_calls: Vec<PersistedToolCall>,
    pub context_snapshot: Option<ContextSnapshotRequest>,
}

/// Request to atomically replace all messages in a conversation
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplaceConversationMessagesRequest {
    pub conversation_id: String,
    pub title: String,
    pub message: SaveMessageRequest,
}

/// Context snapshot from frontend
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextSnapshotRequest {
    pub cwd: Option<String>,
    pub selection: Option<String>,
    pub buffer_tail: Option<String>,
    pub local_os: Option<String>,
    pub connection_info: Option<String>,
    pub terminal_type: Option<String>,
}

/// Response for conversation list
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationListResponse {
    pub conversations: Vec<ConversationMetaResponse>,
}

/// Conversation metadata for list display
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMetaResponse {
    pub id: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub message_count: usize,
    pub session_id: Option<String>,
    pub origin: String,
}

/// Full conversation response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FullConversationResponse {
    pub id: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub session_id: Option<String>,
    pub origin: String,
    pub messages: Vec<MessageResponse>,
}

/// Message response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageResponse {
    pub id: String,
    pub role: String,
    pub content: String,
    pub timestamp: i64,
    #[serde(default)]
    pub tool_calls: Vec<PersistedToolCall>,
    pub context: Option<String>, // Simplified: just the buffer_tail for display
}

/// Stats response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiChatStatsResponse {
    pub conversation_count: usize,
    pub message_count: usize,
}

// ═══════════════════════════════════════════════════════════════════════════
// Error Handling
// ═══════════════════════════════════════════════════════════════════════════

impl From<AiChatError> for String {
    fn from(e: AiChatError) -> Self {
        e.to_string()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tauri Commands
// ═══════════════════════════════════════════════════════════════════════════

/// Extract the AI chat store from optional state, returning an error if unavailable.
fn require_ai_chat_store<'a>(
    state: &'a State<'_, Option<Arc<AiChatStore>>>,
) -> Result<&'a Arc<AiChatStore>, String> {
    state
        .as_ref()
        .ok_or_else(|| "AI chat store not available. Chat persistence is disabled.".to_string())
}

/// List all conversations (metadata only, for sidebar display)
#[tauri::command]
pub async fn ai_chat_list_conversations(
    store: State<'_, Option<Arc<AiChatStore>>>,
) -> Result<ConversationListResponse, String> {
    let store = require_ai_chat_store(&store)?;
    let conversations = store.list_conversations().map_err(|e| e.to_string())?;

    let response = ConversationListResponse {
        conversations: conversations
            .into_iter()
            .map(|m| ConversationMetaResponse {
                id: m.id,
                title: m.title,
                created_at: m.created_at,
                updated_at: m.updated_at,
                message_count: m.message_count,
                session_id: m.session_id,
                origin: m.origin,
            })
            .collect(),
    };

    Ok(response)
}

/// Get a full conversation with all messages
#[tauri::command]
pub async fn ai_chat_get_conversation(
    store: State<'_, Option<Arc<AiChatStore>>>,
    conversation_id: String,
) -> Result<FullConversationResponse, String> {
    let store = require_ai_chat_store(&store)?;
    let full = store
        .get_conversation(&conversation_id)
        .map_err(|e| e.to_string())?;

    let response = FullConversationResponse {
        id: full.meta.id,
        title: full.meta.title,
        created_at: full.meta.created_at,
        updated_at: full.meta.updated_at,
        session_id: full.meta.session_id,
        origin: full.meta.origin,
        messages: full
            .messages
            .into_iter()
            .map(|m| MessageResponse {
                id: m.id,
                role: m.role,
                content: m.content,
                timestamp: m.timestamp,
                tool_calls: m.tool_calls,
                // Return buffer_tail as context for compatibility
                context: m.context_snapshot.and_then(|c| c.buffer_tail),
            })
            .collect(),
    };

    Ok(response)
}

/// Create a new conversation
#[tauri::command]
pub async fn ai_chat_create_conversation(
    store: State<'_, Option<Arc<AiChatStore>>>,
    request: CreateConversationRequest,
) -> Result<(), String> {
    let store = require_ai_chat_store(&store)?;
    let now = chrono::Utc::now().timestamp_millis();

    let meta = ConversationMeta {
        id: request.id,
        title: request.title,
        created_at: now,
        updated_at: now,
        message_count: 0,
        session_id: request.session_id,
        origin: request.origin,
    };

    store.create_conversation(&meta).map_err(|e| e.to_string())
}

/// Update conversation metadata (e.g., title)
#[tauri::command]
pub async fn ai_chat_update_conversation(
    store: State<'_, Option<Arc<AiChatStore>>>,
    conversation_id: String,
    title: String,
) -> Result<(), String> {
    let store = require_ai_chat_store(&store)?;
    // Get existing conversation first
    let full = store
        .get_conversation(&conversation_id)
        .map_err(|e| e.to_string())?;

    let updated = ConversationMeta {
        id: full.meta.id,
        title,
        created_at: full.meta.created_at,
        updated_at: chrono::Utc::now().timestamp_millis(),
        message_count: full.meta.message_count,
        session_id: full.meta.session_id,
        origin: full.meta.origin,
    };

    store
        .update_conversation(&updated)
        .map_err(|e| e.to_string())
}

/// Delete a conversation
#[tauri::command]
pub async fn ai_chat_delete_conversation(
    store: State<'_, Option<Arc<AiChatStore>>>,
    conversation_id: String,
) -> Result<(), String> {
    let store = require_ai_chat_store(&store)?;
    store
        .delete_conversation(&conversation_id)
        .map_err(|e| e.to_string())
}

/// Save a message to a conversation
#[tauri::command]
pub async fn ai_chat_save_message(
    store: State<'_, Option<Arc<AiChatStore>>>,
    request: SaveMessageRequest,
) -> Result<(), String> {
    let store = require_ai_chat_store(&store)?;
    let message = PersistedMessage {
        id: request.id,
        conversation_id: request.conversation_id,
        role: request.role,
        content: request.content,
        timestamp: request.timestamp,
        tool_calls: request.tool_calls,
        context_snapshot: request.context_snapshot.map(|c| ContextSnapshot {
            cwd: c.cwd,
            selection: c.selection,
            buffer_tail: c.buffer_tail,
            buffer_compressed: false, // Will be compressed by save_message if needed
            local_os: c.local_os,
            connection_info: c.connection_info,
            terminal_type: c.terminal_type,
        }),
    };

    store.save_message(message).map_err(|e| e.to_string())
}

/// Update a message content (for streaming updates)
#[tauri::command]
pub async fn ai_chat_update_message(
    store: State<'_, Option<Arc<AiChatStore>>>,
    message_id: String,
    content: String,
) -> Result<(), String> {
    let store = require_ai_chat_store(&store)?;
    store
        .update_message(&message_id, &content)
        .map_err(|e| e.to_string())
}

/// Delete messages after a certain message (for regeneration)
#[tauri::command]
pub async fn ai_chat_delete_messages_after(
    store: State<'_, Option<Arc<AiChatStore>>>,
    conversation_id: String,
    after_message_id: String,
) -> Result<(), String> {
    let store = require_ai_chat_store(&store)?;
    store
        .delete_messages_after(&conversation_id, &after_message_id)
        .map_err(|e| e.to_string())
}

/// Clear all conversations
#[tauri::command]
pub async fn ai_chat_clear_all(store: State<'_, Option<Arc<AiChatStore>>>) -> Result<(), String> {
    let store = require_ai_chat_store(&store)?;
    store.clear_all().map_err(|e| e.to_string())
}

/// Atomically replace all messages in a conversation with a single summary
/// message. Runs inside one redb write transaction — either all changes commit
/// or the original data is preserved.
#[tauri::command]
pub async fn ai_chat_replace_conversation_messages(
    store: State<'_, Option<Arc<AiChatStore>>>,
    request: ReplaceConversationMessagesRequest,
) -> Result<(), String> {
    let store = require_ai_chat_store(&store)?;
    let message = PersistedMessage {
        id: request.message.id,
        // Always use the top-level conversation_id so the message record
        // cannot drift from the target conversation even if the frontend
        // sends mismatched fields.
        conversation_id: request.conversation_id.clone(),
        role: request.message.role,
        content: request.message.content,
        timestamp: request.message.timestamp,
        tool_calls: request.message.tool_calls,
        context_snapshot: request.message.context_snapshot.map(|c| ContextSnapshot {
            cwd: c.cwd,
            selection: c.selection,
            buffer_tail: c.buffer_tail,
            buffer_compressed: false,
            local_os: c.local_os,
            connection_info: c.connection_info,
            terminal_type: c.terminal_type,
        }),
    };

    store
        .replace_conversation_messages(&request.conversation_id, &request.title, message)
        .map_err(|e| e.to_string())
}

/// Get database statistics
#[tauri::command]
pub async fn ai_chat_get_stats(
    store: State<'_, Option<Arc<AiChatStore>>>,
) -> Result<AiChatStatsResponse, String> {
    let store = require_ai_chat_store(&store)?;
    let stats = store.get_stats().map_err(|e| e.to_string())?;

    Ok(AiChatStatsResponse {
        conversation_count: stats.conversation_count,
        message_count: stats.message_count,
    })
}
