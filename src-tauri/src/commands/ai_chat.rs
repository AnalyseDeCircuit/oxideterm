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
    AiChatError, AiChatStore, ContextSnapshot, ConversationMeta, LazyManagedStore,
    PersistedDiagnosticEvent, PersistedMessage, PersistedToolCall, PersistedTranscriptEntry,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
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
    #[serde(default)]
    pub session_metadata: Option<Value>,
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
    #[serde(default)]
    pub turn: Option<Value>,
    #[serde(default)]
    pub transcript_ref: Option<Value>,
    #[serde(default)]
    pub summary_ref: Option<Value>,
}

/// Request to atomically replace all messages in a conversation
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplaceConversationMessagesRequest {
    pub conversation_id: String,
    pub title: String,
    pub message: SaveMessageRequest,
}

/// Request to atomically replace all messages in a conversation with a single
/// new projection message and append transcript entries.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplaceConversationMessagesWithTranscriptRequest {
    pub conversation_id: String,
    pub title: String,
    pub message: SaveMessageRequest,
    #[serde(default)]
    pub transcript_entries: Vec<TranscriptEntryRequest>,
}

/// Request to atomically replace the full ordered message list in a conversation.
/// Used by compaction to avoid intermediate replace + save races.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplaceConversationMessageListRequest {
    pub conversation_id: String,
    pub title: String,
    pub expected_message_ids: Vec<String>,
    pub messages: Vec<SaveMessageRequest>,
}

/// Request to atomically replace the full ordered message list and append
/// transcript entries.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplaceConversationMessageListWithTranscriptRequest {
    pub conversation_id: String,
    pub title: String,
    pub expected_message_ids: Vec<String>,
    pub messages: Vec<SaveMessageRequest>,
    #[serde(default)]
    pub transcript_entries: Vec<TranscriptEntryRequest>,
}

/// Request to append transcript entries for a conversation.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppendTranscriptEntriesRequest {
    pub conversation_id: String,
    pub entries: Vec<TranscriptEntryRequest>,
}

/// Request to append diagnostic events for a conversation.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppendDiagnosticEventsRequest {
    pub conversation_id: String,
    pub events: Vec<DiagnosticEventRequest>,
}

/// Request to atomically save a projection message and append transcript entries.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveMessageWithTranscriptRequest {
    pub message: SaveMessageRequest,
    #[serde(default)]
    pub transcript_entries: Vec<TranscriptEntryRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptEntryRequest {
    pub id: String,
    pub turn_id: Option<String>,
    pub parent_id: Option<String>,
    pub timestamp: i64,
    pub kind: String,
    #[serde(default)]
    pub payload: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticEventRequest {
    pub id: String,
    pub turn_id: Option<String>,
    pub round_id: Option<String>,
    pub timestamp: i64,
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(default)]
    pub data: Value,
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
    pub session_metadata: Option<Value>,
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
    pub session_metadata: Option<Value>,
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
    pub turn: Option<Value>,
    pub transcript_ref: Option<Value>,
    pub summary_ref: Option<Value>,
}

/// Stats response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiChatStatsResponse {
    pub conversation_count: usize,
    pub message_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptResponse {
    pub entries: Vec<TranscriptEntryResponse>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptEntryResponse {
    pub id: String,
    pub conversation_id: String,
    pub turn_id: Option<String>,
    pub parent_id: Option<String>,
    pub timestamp: i64,
    pub kind: String,
    pub payload: Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticTailResponse {
    pub events: Vec<DiagnosticEventResponse>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticEventResponse {
    pub id: String,
    pub conversation_id: String,
    pub turn_id: Option<String>,
    pub round_id: Option<String>,
    pub timestamp: i64,
    #[serde(rename = "type")]
    pub event_type: String,
    pub data: Value,
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
    state: &'a State<'_, LazyManagedStore<AiChatStore>>,
) -> Result<Arc<AiChatStore>, String> {
    state.resolve()
}

/// List all conversations (metadata only, for sidebar display)
#[tauri::command]
pub async fn ai_chat_list_conversations(
    store: State<'_, LazyManagedStore<AiChatStore>>,
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
                session_metadata: m.session_metadata,
            })
            .collect(),
    };

    Ok(response)
}

/// Get a full conversation with all messages
#[tauri::command]
pub async fn ai_chat_get_conversation(
    store: State<'_, LazyManagedStore<AiChatStore>>,
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
        session_metadata: full.meta.session_metadata,
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
                turn: m.turn,
                transcript_ref: m.transcript_ref,
                summary_ref: m.summary_ref,
            })
            .collect(),
    };

    Ok(response)
}

/// Get transcript entries for a conversation.
#[tauri::command]
pub async fn ai_chat_get_transcript(
    store: State<'_, LazyManagedStore<AiChatStore>>,
    conversation_id: String,
) -> Result<TranscriptResponse, String> {
    let store = require_ai_chat_store(&store)?;
    let entries = store
        .get_transcript_entries(&conversation_id)
        .map_err(|e| e.to_string())?;

    Ok(TranscriptResponse {
        entries: entries
            .into_iter()
            .map(|entry| TranscriptEntryResponse {
                id: entry.id,
                conversation_id: entry.conversation_id,
                turn_id: entry.turn_id,
                parent_id: entry.parent_id,
                timestamp: entry.timestamp,
                kind: entry.kind,
                payload: entry.payload,
            })
            .collect(),
    })
}

/// Get the newest diagnostic events for a conversation.
#[tauri::command]
pub async fn ai_chat_get_diagnostic_tail(
    store: State<'_, LazyManagedStore<AiChatStore>>,
    conversation_id: String,
    count: usize,
) -> Result<DiagnosticTailResponse, String> {
    let store = require_ai_chat_store(&store)?;
    let events = store
        .get_diagnostic_events_tail(&conversation_id, count)
        .map_err(|e| e.to_string())?;

    Ok(DiagnosticTailResponse {
        events: events
            .into_iter()
            .map(|event| DiagnosticEventResponse {
                id: event.id,
                conversation_id: event.conversation_id,
                turn_id: event.turn_id,
                round_id: event.round_id,
                timestamp: event.timestamp,
                event_type: event.event_type,
                data: event.data,
            })
            .collect(),
    })
}

/// Create a new conversation
#[tauri::command]
pub async fn ai_chat_create_conversation(
    store: State<'_, LazyManagedStore<AiChatStore>>,
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
        session_metadata: request.session_metadata,
    };

    store.create_conversation(&meta).map_err(|e| e.to_string())
}

/// Update conversation metadata (e.g., title)
#[tauri::command]
pub async fn ai_chat_update_conversation(
    store: State<'_, LazyManagedStore<AiChatStore>>,
    conversation_id: String,
    title: String,
    session_metadata: Option<Value>,
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
        session_metadata: session_metadata.or(full.meta.session_metadata),
    };

    store
        .update_conversation(&updated)
        .map_err(|e| e.to_string())
}

/// Delete a conversation
#[tauri::command]
pub async fn ai_chat_delete_conversation(
    store: State<'_, LazyManagedStore<AiChatStore>>,
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
    store: State<'_, LazyManagedStore<AiChatStore>>,
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
        turn: request.turn,
        transcript_ref: request.transcript_ref,
        summary_ref: request.summary_ref,
    };

    store
        .save_message_with_transcript_entries(message, Vec::new())
        .map_err(|e| e.to_string())
}

/// Append transcript entries for a conversation.
#[tauri::command]
pub async fn ai_chat_append_transcript_entries(
    store: State<'_, LazyManagedStore<AiChatStore>>,
    request: AppendTranscriptEntriesRequest,
) -> Result<(), String> {
    let store = require_ai_chat_store(&store)?;
    let conversation_id = request.conversation_id;
    let entries = request
        .entries
        .into_iter()
        .map(|entry| PersistedTranscriptEntry {
            id: entry.id,
            conversation_id: conversation_id.clone(),
            turn_id: entry.turn_id,
            parent_id: entry.parent_id,
            timestamp: entry.timestamp,
            kind: entry.kind,
            payload: entry.payload,
        })
        .collect::<Vec<_>>();

    store
        .append_transcript_entries(&conversation_id, entries)
        .map_err(|e| e.to_string())
}

/// Append diagnostic events for a conversation.
#[tauri::command]
pub async fn ai_chat_append_diagnostic_events(
    store: State<'_, LazyManagedStore<AiChatStore>>,
    request: AppendDiagnosticEventsRequest,
) -> Result<(), String> {
    let store = require_ai_chat_store(&store)?;
    let conversation_id = request.conversation_id;
    let events = request
        .events
        .into_iter()
        .map(|event| PersistedDiagnosticEvent {
            id: event.id,
            conversation_id: conversation_id.clone(),
            turn_id: event.turn_id,
            round_id: event.round_id,
            timestamp: event.timestamp,
            event_type: event.event_type,
            data: event.data,
        })
        .collect::<Vec<_>>();

    store
        .append_diagnostic_events(&conversation_id, events)
        .map_err(|e| e.to_string())
}

/// Atomically save a message projection and append transcript entries.
#[tauri::command]
pub async fn ai_chat_save_message_with_transcript(
    store: State<'_, LazyManagedStore<AiChatStore>>,
    request: SaveMessageWithTranscriptRequest,
) -> Result<(), String> {
    let store = require_ai_chat_store(&store)?;

    let message = PersistedMessage {
        id: request.message.id,
        conversation_id: request.message.conversation_id.clone(),
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
        turn: request.message.turn,
        transcript_ref: request.message.transcript_ref,
        summary_ref: request.message.summary_ref,
    };

    let entries = request
        .transcript_entries
        .into_iter()
        .map(|entry| PersistedTranscriptEntry {
            id: entry.id,
            conversation_id: message.conversation_id.clone(),
            turn_id: entry.turn_id,
            parent_id: entry.parent_id,
            timestamp: entry.timestamp,
            kind: entry.kind,
            payload: entry.payload,
        })
        .collect::<Vec<_>>();

    store
        .save_message_with_transcript_entries(message, entries)
        .map_err(|e| e.to_string())
}

/// Update a message content (for streaming updates)
#[tauri::command]
pub async fn ai_chat_update_message(
    store: State<'_, LazyManagedStore<AiChatStore>>,
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
    store: State<'_, LazyManagedStore<AiChatStore>>,
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
pub async fn ai_chat_clear_all(
    store: State<'_, LazyManagedStore<AiChatStore>>,
) -> Result<(), String> {
    let store = require_ai_chat_store(&store)?;
    store.clear_all().map_err(|e| e.to_string())
}

/// Atomically replace all messages in a conversation with a single summary
/// message. Runs inside one redb write transaction — either all changes commit
/// or the original data is preserved.
#[tauri::command]
pub async fn ai_chat_replace_conversation_messages(
    store: State<'_, LazyManagedStore<AiChatStore>>,
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
        turn: request.message.turn,
        transcript_ref: request.message.transcript_ref,
        summary_ref: request.message.summary_ref,
    };

    store
        .replace_conversation_messages(&request.conversation_id, &request.title, message)
        .map_err(|e| e.to_string())
}

/// Atomically replace all messages in a conversation with a single summary
/// message and append transcript entries in the same transaction.
#[tauri::command]
pub async fn ai_chat_replace_conversation_messages_with_transcript(
    store: State<'_, LazyManagedStore<AiChatStore>>,
    request: ReplaceConversationMessagesWithTranscriptRequest,
) -> Result<(), String> {
    let store = require_ai_chat_store(&store)?;
    let message = PersistedMessage {
        id: request.message.id,
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
        turn: request.message.turn,
        transcript_ref: request.message.transcript_ref,
        summary_ref: request.message.summary_ref,
    };

    let entries = request
        .transcript_entries
        .into_iter()
        .map(|entry| PersistedTranscriptEntry {
            id: entry.id,
            conversation_id: request.conversation_id.clone(),
            turn_id: entry.turn_id,
            parent_id: entry.parent_id,
            timestamp: entry.timestamp,
            kind: entry.kind,
            payload: entry.payload,
        })
        .collect::<Vec<_>>();

    store
        .replace_conversation_messages_with_transcript_entries(
            &request.conversation_id,
            &request.title,
            message,
            entries,
        )
        .map_err(|e| e.to_string())
}

/// Atomically replace the entire ordered message list for a conversation.
/// The operation only commits if the current persisted message ids still match
/// the expected ids sent by the frontend, preventing stale compaction writes.
#[tauri::command]
pub async fn ai_chat_replace_conversation_message_list(
    store: State<'_, LazyManagedStore<AiChatStore>>,
    request: ReplaceConversationMessageListRequest,
) -> Result<(), String> {
    let store = require_ai_chat_store(&store)?;
    let messages = request
        .messages
        .into_iter()
        .map(|message| PersistedMessage {
            id: message.id,
            conversation_id: request.conversation_id.clone(),
            role: message.role,
            content: message.content,
            timestamp: message.timestamp,
            tool_calls: message.tool_calls,
            context_snapshot: message.context_snapshot.map(|c| ContextSnapshot {
                cwd: c.cwd,
                selection: c.selection,
                buffer_tail: c.buffer_tail,
                buffer_compressed: false,
                local_os: c.local_os,
                connection_info: c.connection_info,
                terminal_type: c.terminal_type,
            }),
            turn: message.turn,
            transcript_ref: message.transcript_ref,
            summary_ref: message.summary_ref,
        })
        .collect::<Vec<_>>();

    store
        .replace_conversation_message_list(
            &request.conversation_id,
            &request.title,
            &request.expected_message_ids,
            messages,
        )
        .map_err(|e| e.to_string())
}

/// Atomically replace the entire ordered message list for a conversation and
/// append transcript entries in the same transaction.
#[tauri::command]
pub async fn ai_chat_replace_conversation_message_list_with_transcript(
    store: State<'_, LazyManagedStore<AiChatStore>>,
    request: ReplaceConversationMessageListWithTranscriptRequest,
) -> Result<(), String> {
    let store = require_ai_chat_store(&store)?;
    let messages = request
        .messages
        .into_iter()
        .map(|message| PersistedMessage {
            id: message.id,
            conversation_id: request.conversation_id.clone(),
            role: message.role,
            content: message.content,
            timestamp: message.timestamp,
            tool_calls: message.tool_calls,
            context_snapshot: message.context_snapshot.map(|c| ContextSnapshot {
                cwd: c.cwd,
                selection: c.selection,
                buffer_tail: c.buffer_tail,
                buffer_compressed: false,
                local_os: c.local_os,
                connection_info: c.connection_info,
                terminal_type: c.terminal_type,
            }),
            turn: message.turn,
            transcript_ref: message.transcript_ref,
            summary_ref: message.summary_ref,
        })
        .collect::<Vec<_>>();

    let entries = request
        .transcript_entries
        .into_iter()
        .map(|entry| PersistedTranscriptEntry {
            id: entry.id,
            conversation_id: request.conversation_id.clone(),
            turn_id: entry.turn_id,
            parent_id: entry.parent_id,
            timestamp: entry.timestamp,
            kind: entry.kind,
            payload: entry.payload,
        })
        .collect::<Vec<_>>();

    store
        .replace_conversation_message_list_with_transcript_entries(
            &request.conversation_id,
            &request.title,
            &request.expected_message_ids,
            messages,
            entries,
        )
        .map_err(|e| e.to_string())
}

/// Get database statistics
#[tauri::command]
pub async fn ai_chat_get_stats(
    store: State<'_, LazyManagedStore<AiChatStore>>,
) -> Result<AiChatStatsResponse, String> {
    let store = require_ai_chat_store(&store)?;
    let stats = store.get_stats().map_err(|e| e.to_string())?;

    Ok(AiChatStatsResponse {
        conversation_count: stats.conversation_count,
        message_count: stats.message_count,
    })
}
