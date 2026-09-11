use std::collections::HashMap;

use serde_json::{Value, json};

use crate::{AiChatMessage, AiChatRole, AiChatStreamConfig, AiToolCall, AiToolChoice};

pub(crate) fn responses_body(config: &AiChatStreamConfig, messages: &[AiChatMessage]) -> Value {
    let mut input = Vec::new();
    let mut call_ids = HashMap::<String, String>::new();
    let scope = config.response_state_key();
    for message in messages {
        if message.role == AiChatRole::Assistant
            && let Some(rounds) = crate::ai_provider_parts(message, &scope)
        {
            for round in rounds {
                if let Some(ids) = round.get("callIds").and_then(Value::as_object) {
                    for (local, wire) in ids {
                        if let Some(wire) = wire.as_str() {
                            call_ids.insert(local.clone(), wire.to_owned());
                        }
                    }
                }
                if let Some(output) = round.get("output").and_then(Value::as_array) {
                    input.extend(output.iter().cloned());
                }
                if let Some(results) = round.get("results").and_then(Value::as_array) {
                    input.extend(results.iter().cloned());
                }
            }
            continue;
        }
        match message.role {
            AiChatRole::Tool => {
                if let Some(id) = &message.tool_call_id {
                    input.push(json!({
                        "type": "function_call_output",
                        "call_id": call_ids.get(id).unwrap_or(id),
                        "output": message.content,
                    }));
                }
            }
            role => {
                if !message.content.is_empty() {
                    let role = match role {
                        AiChatRole::User => "user",
                        AiChatRole::Assistant => "assistant",
                        _ => "system",
                    };
                    input.push(json!({"role": role, "content": message.content}));
                }
                for call in message.tool_calls.iter().filter_map(AiToolCall::from_value) {
                    input.push(json!({
                        "type": "function_call",
                        "call_id": call.id,
                        "name": call.name,
                        "arguments": call.arguments,
                    }));
                }
            }
        }
    }
    let mut body = json!({
        "model": config.model,
        "input": input,
        "stream": true,
        "store": false,
        "include": ["reasoning.encrypted_content"],
    });
    if let Some(tokens) = config.max_response_tokens.filter(|tokens| *tokens > 0) {
        body["max_output_tokens"] = json!(tokens);
    }
    if let Some(effort) = config
        .reasoning_effort
        .as_deref()
        .filter(|effort| *effort != "auto")
    {
        body["reasoning"] = json!({"effort": effort});
    }
    // Compatible gateways use the same OpenAI model names; provider branding must not hide summaries.
    let capability = crate::model_reasoning_capability("openai", &config.model);
    let explicit_reasoning = config.reasoning_effort.as_deref()
        .is_some_and(|effort| !matches!(effort, "auto" | "none"));
    if (capability.known_model && capability.request_format == crate::AiReasoningRequestFormat::OpenAi)
        || (!capability.known_model && explicit_reasoning)
    {
        body["reasoning"]["summary"] = json!("auto");
    }
    if !config.tools.is_empty() {
        body["tools"] = Value::Array(
            config
                .tools
                .iter()
                .map(|tool| {
                    json!({
                        "type": "function",
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.parameters,
                        "strict": false,
                    })
                })
                .collect(),
        );
        body["tool_choice"] = match &config.tool_choice {
            AiToolChoice::Auto => json!("auto"),
            AiToolChoice::Required => json!("required"),
            AiToolChoice::Named(name) => json!({"type":"function", "name":name}),
        };
    }
    body
}
