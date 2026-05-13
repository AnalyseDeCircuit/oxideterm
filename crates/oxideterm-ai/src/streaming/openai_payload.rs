use serde_json::Value;

use crate::{AiChatMessage, AiChatRole, AiChatStreamConfig};

pub(crate) fn openai_chat_body(config: &AiChatStreamConfig, messages: &[AiChatMessage]) -> Value {
    let mut body = serde_json::json!({
        "model": config.model,
        "messages": openai_chat_messages(messages),
        "stream": true,
    });
    if let Some(tokens) = config.max_response_tokens.filter(|tokens| *tokens > 0)
        && let Some(object) = body.as_object_mut()
    {
        object.insert("max_tokens".to_string(), serde_json::json!(tokens));
    }
    body
}

pub(crate) fn openai_chat_messages(messages: &[AiChatMessage]) -> Vec<Value> {
    let mut system_parts = Vec::new();
    let mut out = Vec::new();
    for message in messages {
        match message.role {
            AiChatRole::System if !message.content.is_empty() => {
                system_parts.push(message.content.clone());
            }
            AiChatRole::System => {}
            AiChatRole::User | AiChatRole::Assistant => out.push(serde_json::json!({
                "role": if message.role == AiChatRole::User { "user" } else { "assistant" },
                "content": message.content,
            })),
        }
    }
    if !system_parts.is_empty() {
        out.insert(
            0,
            serde_json::json!({
                "role": "system",
                "content": system_parts.join("\n\n"),
            }),
        );
    }
    out
}
