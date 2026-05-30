use serde_json::Value;

use crate::{
    AiChatMessage, AiChatRole, AiChatStreamConfig, AiToolCall, AiToolChoice, AiToolDefinition,
};

pub(crate) fn openai_chat_body(config: &AiChatStreamConfig, messages: &[AiChatMessage]) -> Value {
    let mut body = serde_json::json!({
        "model": config.model,
        "messages": openai_chat_messages(config, messages),
        "stream": true,
    });
    if let Some(tokens) = config.max_response_tokens.filter(|tokens| *tokens > 0)
        && let Some(object) = body.as_object_mut()
    {
        object.insert("max_tokens".to_string(), serde_json::json!(tokens));
    }
    if let Some(object) = body.as_object_mut() {
        apply_reasoning_options(object, config);
        apply_tool_options(object, config);
    }
    body
}

fn apply_tool_options(body: &mut serde_json::Map<String, Value>, config: &AiChatStreamConfig) {
    if config.tools.is_empty() {
        return;
    }
    body.insert(
        "tools".to_string(),
        serde_json::json!(openai_tool_definitions(&config.tools)),
    );
    match &config.tool_choice {
        AiToolChoice::Auto => {}
        AiToolChoice::Required => {
            body.insert("tool_choice".to_string(), serde_json::json!("required"));
        }
        AiToolChoice::Named(name) if !name.is_empty() => {
            body.insert(
                "tool_choice".to_string(),
                serde_json::json!({
                    "type": "function",
                    "function": { "name": name },
                }),
            );
        }
        AiToolChoice::Named(_) => {}
    }
}

fn openai_tool_definitions(tools: &[AiToolDefinition]) -> Vec<Value> {
    tools
        .iter()
        .map(|tool| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.parameters,
                },
            })
        })
        .collect()
}

fn apply_reasoning_options(body: &mut serde_json::Map<String, Value>, config: &AiChatStreamConfig) {
    let effort = config.reasoning_effort.as_deref().unwrap_or("auto");
    if effort == "auto" {
        return;
    }

    match config.provider_type.as_str() {
        "deepseek" => {
            if matches!(effort, "off" | "none") {
                body.insert(
                    "thinking".to_string(),
                    serde_json::json!({ "type": "disabled" }),
                );
                return;
            }
            body.insert(
                "thinking".to_string(),
                serde_json::json!({ "type": "enabled" }),
            );
            body.insert(
                "reasoning_effort".to_string(),
                serde_json::json!(if matches!(effort, "max" | "xhigh") {
                    "max"
                } else {
                    "high"
                }),
            );
        }
        "openai" => {
            let value = match effort {
                "off" | "none" => "minimal",
                "max" | "xhigh" => "high",
                "minimal" | "low" | "medium" | "high" => effort,
                _ => return,
            };
            body.insert("reasoning_effort".to_string(), serde_json::json!(value));
        }
        _ => {}
    }
}

pub(crate) fn openai_chat_messages(
    config: &AiChatStreamConfig,
    messages: &[AiChatMessage],
) -> Vec<Value> {
    let mut system_parts = Vec::new();
    let mut non_system = Vec::new();
    for message in messages {
        match message.role {
            AiChatRole::System if !message.content.is_empty() => {
                system_parts.push(message.content.clone());
            }
            AiChatRole::System => {}
            _ => non_system.push(message),
        }
    }

    // Tauri only normalizes system messages when there is at least one
    // non-empty system prompt; all-empty system messages are sent as-is.
    let normalized = if system_parts.is_empty() {
        messages.iter().collect::<Vec<_>>()
    } else {
        non_system
    };

    let last_user_index = normalized
        .iter()
        .rposition(|message| message.role == AiChatRole::User)
        .unwrap_or(usize::MAX);
    let mut out = normalized
        .iter()
        .enumerate()
        .map(|(index, message)| openai_message_value(config, message, index, last_user_index))
        .collect::<Vec<_>>();
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

fn openai_message_value(
    config: &AiChatStreamConfig,
    message: &AiChatMessage,
    index: usize,
    last_user_index: usize,
) -> Value {
    match message.role {
        AiChatRole::User => serde_json::json!({
            "role": "user",
            "content": message.content,
        }),
        AiChatRole::System => serde_json::json!({
            "role": "system",
            "content": message.content,
        }),
        AiChatRole::Tool => {
            let mut tool = serde_json::json!({
                "role": "tool",
                "content": message.content,
            });
            if let Some(tool_call_id) = message.tool_call_id.as_ref()
                && let Some(object) = tool.as_object_mut()
            {
                object.insert(
                    "tool_call_id".to_string(),
                    Value::String(tool_call_id.clone()),
                );
            }
            tool
        }
        AiChatRole::Assistant => {
            let calls = tool_calls_from_message(message);
            if calls.is_empty() {
                serde_json::json!({
                    "role": "assistant",
                    "content": message.content,
                })
            } else {
                let mut assistant = serde_json::json!({
                    "role": "assistant",
                    "content": if message.content.is_empty() {
                        Value::Null
                    } else {
                        Value::String(message.content.clone())
                    },
                    "tool_calls": calls
                        .into_iter()
                        .map(|call| serde_json::json!({
                            "id": call.id,
                            "type": "function",
                            "function": {
                                "name": call.name,
                                "arguments": call.arguments,
                            },
                        }))
                        .collect::<Vec<_>>(),
                });
                if let Some(reasoning) = message.thinking_content.as_ref()
                    && should_preserve_reasoning_content(config, index, last_user_index)
                    && let Some(object) = assistant.as_object_mut()
                {
                    object.insert(
                        "reasoning_content".to_string(),
                        Value::String(reasoning.clone()),
                    );
                }
                assistant
            }
        }
    }
}

fn should_preserve_reasoning_content(
    config: &AiChatStreamConfig,
    index: usize,
    last_user_index: usize,
) -> bool {
    config.provider_type != "deepseek" || index > last_user_index
}

fn tool_calls_from_message(message: &AiChatMessage) -> Vec<AiToolCall> {
    message
        .tool_calls
        .iter()
        .filter_map(AiToolCall::from_value)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AiPolicySafetyMode, AiToolUsePolicy};

    fn message(role: AiChatRole, content: &str) -> AiChatMessage {
        AiChatMessage {
            id: format!("message-{content}"),
            role,
            content: content.to_string(),
            timestamp_ms: 1,
            model: None,
            context: None,
            thinking_content: None,
            is_streaming: false,
            metadata: None,
            tool_call_id: None,
            tool_calls: Vec::new(),
            turn: None,
            transcript_ref: None,
            summary_ref: None,
            branches: None,
            suggestions: Vec::new(),
        }
    }

    fn config(provider_type: &str, reasoning_effort: &str) -> AiChatStreamConfig {
        AiChatStreamConfig {
            provider_id: Some("provider".to_string()),
            provider_type: provider_type.to_string(),
            base_url: "https://api.example.test".to_string(),
            model: "model".to_string(),
            api_key: None,
            max_response_tokens: None,
            reasoning_effort: Some(reasoning_effort.to_string()),
            safety_mode: AiPolicySafetyMode::Default,
            profile_id: None,
            tool_policy: AiToolUsePolicy::default(),
            tools: Vec::new(),
            tool_choice: AiToolChoice::Auto,
        }
    }

    #[test]
    fn openai_reasoning_payload_matches_tauri_mapping() {
        let body = openai_chat_body(&config("openai", "xhigh"), &[]);
        assert_eq!(body["reasoning_effort"].as_str(), Some("high"));

        let body = openai_chat_body(&config("openai", "off"), &[]);
        assert_eq!(body["reasoning_effort"].as_str(), Some("minimal"));
    }

    #[test]
    fn deepseek_reasoning_payload_matches_tauri_mapping() {
        let body = openai_chat_body(&config("deepseek", "none"), &[]);
        assert_eq!(body["thinking"]["type"].as_str(), Some("disabled"));

        let body = openai_chat_body(&config("deepseek", "max"), &[]);
        assert_eq!(body["thinking"]["type"].as_str(), Some("enabled"));
        assert_eq!(body["reasoning_effort"].as_str(), Some("max"));
    }

    #[test]
    fn openai_tool_payload_matches_tauri_shape() {
        let mut config = config("openai", "auto");
        config.tools = vec![AiToolDefinition {
            name: "run_command".to_string(),
            description: "Run command".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string" }
                },
                "required": ["command"],
            }),
        }];
        config.tool_choice = AiToolChoice::Named("run_command".to_string());

        let body = openai_chat_body(&config, &[]);
        assert_eq!(body["tools"][0]["type"].as_str(), Some("function"));
        assert_eq!(
            body["tools"][0]["function"]["name"].as_str(),
            Some("run_command")
        );
        assert_eq!(
            body["tool_choice"]["function"]["name"].as_str(),
            Some("run_command")
        );

        config.tool_choice = AiToolChoice::Required;
        let body = openai_chat_body(&config, &[]);
        assert_eq!(body["tool_choice"].as_str(), Some("required"));
    }

    #[test]
    fn openai_message_conversion_preserves_tool_calls_and_results() {
        let assistant = AiChatMessage {
            id: "a1".to_string(),
            role: AiChatRole::Assistant,
            content: String::new(),
            timestamp_ms: 1,
            model: None,
            context: None,
            thinking_content: None,
            is_streaming: false,
            metadata: None,
            tool_call_id: None,
            tool_calls: vec![serde_json::json!({
                "id": "call-1",
                "name": "run_command",
                "arguments": "{\"command\":\"pwd\"}",
            })],
            turn: None,
            transcript_ref: None,
            summary_ref: None,
            branches: None,
            suggestions: Vec::new(),
        };
        let result = AiChatMessage {
            id: "t1".to_string(),
            role: AiChatRole::Tool,
            content: "{\"ok\":true}".to_string(),
            timestamp_ms: 2,
            model: None,
            context: None,
            thinking_content: None,
            is_streaming: false,
            metadata: None,
            tool_call_id: Some("call-1".to_string()),
            tool_calls: Vec::new(),
            turn: None,
            transcript_ref: None,
            summary_ref: None,
            branches: None,
            suggestions: Vec::new(),
        };

        let converted = openai_chat_messages(&config("openai", "auto"), &[assistant, result]);
        assert_eq!(converted[0]["role"].as_str(), Some("assistant"));
        assert!(converted[0]["content"].is_null());
        assert_eq!(
            converted[0]["tool_calls"][0]["function"]["arguments"].as_str(),
            Some("{\"command\":\"pwd\"}")
        );
        assert_eq!(converted[1]["role"].as_str(), Some("tool"));
        assert_eq!(converted[1]["tool_call_id"].as_str(), Some("call-1"));
    }

    #[test]
    fn openai_tool_message_omits_missing_call_id_like_tauri_json() {
        let tool = message(AiChatRole::Tool, "{\"ok\":true}");
        let converted = openai_chat_messages(&config("openai", "auto"), &[tool]);

        assert_eq!(converted[0]["role"].as_str(), Some("tool"));
        assert_eq!(converted[0]["content"].as_str(), Some("{\"ok\":true}"));
        assert!(converted[0].get("tool_call_id").is_none());
    }

    #[test]
    fn openai_empty_system_message_branch_matches_tauri_merge_semantics() {
        let config = config("openai", "auto");
        let converted = openai_chat_messages(
            &config,
            &[
                message(AiChatRole::System, ""),
                message(AiChatRole::User, "hello"),
            ],
        );
        assert_eq!(converted[0]["role"].as_str(), Some("system"));
        assert_eq!(converted[0]["content"].as_str(), Some(""));
        assert_eq!(converted[1]["role"].as_str(), Some("user"));

        let converted = openai_chat_messages(
            &config,
            &[
                message(AiChatRole::System, ""),
                message(AiChatRole::System, "real system"),
                message(AiChatRole::User, "hello"),
            ],
        );
        assert_eq!(converted.len(), 2);
        assert_eq!(converted[0]["role"].as_str(), Some("system"));
        assert_eq!(converted[0]["content"].as_str(), Some("real system"));
        assert_eq!(converted[1]["role"].as_str(), Some("user"));
    }
}
