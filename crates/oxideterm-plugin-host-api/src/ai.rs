// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::HashMap;

use oxideterm_plugin_protocol as plugin_runtime;
use serde_json::{Map, Value, json};

// AI host APIs expose sanitized conversation snapshots only; provider secrets,
// tool messages, and backend-only state stay outside the plugin contract.
pub fn native_plugin_ai_response(
    call: plugin_runtime::PluginHostCall,
    snapshot: &Value,
) -> plugin_runtime::PluginResponse {
    let request_id = call.request_id.clone();
    match call.method.as_str() {
        "getConversations" => plugin_runtime::PluginResponse::ok(
            request_id,
            snapshot
                .get("conversations")
                .cloned()
                .unwrap_or_else(|| json!([])),
        ),
        "getMessages" => {
            let Some(conversation_id) = call.args.get("conversationId").and_then(Value::as_str)
            else {
                return plugin_runtime::PluginResponse::error(
                    request_id,
                    plugin_runtime::PluginError::protocol(
                        "invalid_ai_conversation",
                        "ai.getMessages requires args.conversationId",
                    ),
                );
            };
            let messages = snapshot
                .get("messagesByConversation")
                .and_then(|messages| messages.get(conversation_id))
                .cloned()
                .unwrap_or_else(|| json!([]));
            plugin_runtime::PluginResponse::ok(request_id, messages)
        }
        "getActiveProvider" => plugin_runtime::PluginResponse::ok(
            request_id,
            snapshot
                .get("activeProvider")
                .cloned()
                .unwrap_or(Value::Null),
        ),
        "getAvailableModels" => plugin_runtime::PluginResponse::ok(
            request_id,
            snapshot
                .get("availableModels")
                .cloned()
                .unwrap_or_else(|| json!([])),
        ),
        "onMessage" => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime(
                "plugin_ai_subscription_bridge",
                "AI subscriptions are registered through the runtime event bridge",
            ),
        ),
        method => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::protocol(
                "unknown_ai_method",
                format!("Unknown ai.{method} host API"),
            ),
        ),
    }
}

pub fn native_plugin_ai_snapshot_value(
    chat: &oxideterm_ai::AiChatState,
    providers: &[Value],
    active_provider_id: Option<&str>,
    model_context_windows: &Map<String, Value>,
) -> Value {
    let provider_views = oxideterm_ai::provider_views(providers);
    let active_provider = oxideterm_ai::active_provider_view(&provider_views, active_provider_id);
    let active_provider_value = active_provider.map(|provider| {
        json!({
            "type": provider.provider_type,
            "displayName": provider.name,
        })
    });
    let available_models = active_provider_id
        .and_then(|provider_id| model_context_windows.get(provider_id))
        .and_then(Value::as_object)
        .map(|models| {
            models
                .keys()
                .cloned()
                .map(Value::String)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut messages_by_conversation = Map::new();
    for conversation in &chat.conversations {
        messages_by_conversation.insert(
            conversation.id.clone(),
            Value::Array(
                conversation
                    .messages
                    .iter()
                    .filter_map(native_plugin_ai_message_snapshot)
                    .collect(),
            ),
        );
    }
    json!({
        "conversations": chat
            .conversations
            .iter()
            .map(native_plugin_ai_conversation_snapshot)
            .collect::<Vec<_>>(),
        "messagesByConversation": messages_by_conversation,
        "activeProvider": active_provider_value,
        "availableModels": available_models,
    })
}

fn native_plugin_ai_conversation_snapshot(conversation: &oxideterm_ai::AiConversation) -> Value {
    // The plugin API does not expose tool-role messages, so the count follows
    // the sanitized message projection used by getMessages and onMessage.
    let visible_message_count = conversation
        .messages
        .iter()
        .filter_map(native_plugin_ai_message_snapshot)
        .count();
    json!({
        "id": &conversation.id,
        "title": &conversation.title,
        "messageCount": visible_message_count,
        "createdAt": conversation.created_at_ms,
        "updatedAt": conversation.updated_at_ms,
    })
}

fn native_plugin_ai_message_snapshot(message: &oxideterm_ai::AiChatMessage) -> Option<Value> {
    let role = native_plugin_ai_role_label(message.role)?;
    Some(json!({
        "id": &message.id,
        "role": role,
        "content": oxideterm_ai::sanitize_for_ai(&message.content),
        "timestamp": message.timestamp_ms,
    }))
}

fn native_plugin_ai_role_label(role: oxideterm_ai::AiChatRole) -> Option<&'static str> {
    match role {
        oxideterm_ai::AiChatRole::User => Some("user"),
        oxideterm_ai::AiChatRole::Assistant => Some("assistant"),
        oxideterm_ai::AiChatRole::System => Some("system"),
        oxideterm_ai::AiChatRole::Tool => None,
    }
}

pub fn native_plugin_ai_message_count_map(snapshot: &Value) -> HashMap<String, usize> {
    snapshot
        .get("conversations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|conversation| {
            let id = conversation.get("id").and_then(Value::as_str)?;
            let count = conversation
                .get("messageCount")
                .and_then(Value::as_u64)
                .unwrap_or_default() as usize;
            Some((id.to_string(), count))
        })
        .collect()
}

pub fn native_plugin_ai_new_message_events(
    snapshot: &Value,
    previous_counts: &HashMap<String, usize>,
) -> Vec<Value> {
    let Some(conversations) = snapshot.get("conversations").and_then(Value::as_array) else {
        return Vec::new();
    };
    conversations
        .iter()
        .filter_map(|conversation| {
            let conversation_id = conversation.get("id").and_then(Value::as_str)?;
            let count = conversation
                .get("messageCount")
                .and_then(Value::as_u64)
                .unwrap_or_default() as usize;
            if count
                <= previous_counts
                    .get(conversation_id)
                    .copied()
                    .unwrap_or_default()
            {
                return None;
            }
            let message = snapshot
                .get("messagesByConversation")
                .and_then(|messages| messages.get(conversation_id))
                .and_then(Value::as_array)
                .and_then(|messages| messages.last())?;
            Some(json!({
                "conversationId": conversation_id,
                "messageId": message.get("id").and_then(Value::as_str).unwrap_or_default(),
                "role": message.get("role").and_then(Value::as_str).unwrap_or_default(),
            }))
        })
        .collect()
}
