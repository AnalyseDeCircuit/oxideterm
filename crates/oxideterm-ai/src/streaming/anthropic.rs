use anyhow::{Context, Result, anyhow};
use serde_json::Value;

use crate::providers::{ANTHROPIC_VERSION, api_key_required_ref};
use crate::{
    AiChatMessage, AiChatRole, AiChatStreamConfig, AiStreamEvent, AiToolCall, AiToolChoice,
    AiToolDefinition,
};

use super::CHAT_STREAM_TIMEOUT;
use super::common::{ParsedStreamLine, stream_sse_response};

pub(crate) async fn stream_anthropic_completion(
    config: AiChatStreamConfig,
    messages: Vec<AiChatMessage>,
    events: tokio::sync::mpsc::UnboundedSender<AiStreamEvent>,
) -> Result<()> {
    let api_key = api_key_required_ref(&config.provider_type, config.api_key.as_ref())?;
    let url = format!(
        "{}/v1/messages",
        config.base_url.trim().trim_end_matches('/')
    );
    let client = reqwest::Client::builder()
        .timeout(CHAT_STREAM_TIMEOUT)
        .build()
        .context("failed to create Anthropic chat client")?;
    let body = anthropic_chat_body(&config, &messages);
    let response = client
        .post(&url)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header("x-api-key", api_key.as_str())
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&body)
        .send()
        .await
        .map_err(|error| {
            anyhow!(
                "failed to connect to Anthropic provider: {}",
                error.without_url()
            )
        })?;
    if !response.status().is_success() {
        let status = response.status().as_u16();
        let error_text = response.text().await.unwrap_or_default();
        return Err(anyhow!(parse_anthropic_error(status, &error_text)));
    }
    let mut accumulator = AnthropicToolAccumulator::default();
    let _ = stream_sse_response(response, &events, |line| {
        parse_anthropic_data_line_with_accumulator(line, &mut accumulator)
    })
    .await?;
    let _ = events.send(AiStreamEvent::Done);
    Ok(())
}

fn anthropic_chat_body(config: &AiChatStreamConfig, messages: &[AiChatMessage]) -> Value {
    let (system, api_messages) = anthropic_chat_messages(messages);
    let mut body = serde_json::json!({
        "model": config.model,
        "messages": api_messages,
        "max_tokens": config.max_response_tokens.unwrap_or(8192).max(1),
        "stream": true,
    });
    let thinking_budget = anthropic_thinking_budget(config);
    if let Some(thinking_budget) = thinking_budget
        && let Some(object) = body.as_object_mut()
    {
        object.insert(
            "thinking".to_string(),
            serde_json::json!({ "type": "enabled", "budget_tokens": thinking_budget }),
        );
    }
    if let Some(system) = system
        && let Some(object) = body.as_object_mut()
    {
        object.insert("system".to_string(), serde_json::json!(system));
    }
    if let Some(object) = body.as_object_mut() {
        apply_anthropic_tool_options(object, config, thinking_budget.is_some());
    }
    body
}

fn apply_anthropic_tool_options(
    body: &mut serde_json::Map<String, Value>,
    config: &AiChatStreamConfig,
    thinking_enabled: bool,
) {
    if config.tools.is_empty() {
        return;
    }
    body.insert(
        "tools".to_string(),
        serde_json::json!(anthropic_tool_definitions(&config.tools)),
    );
    if thinking_enabled {
        return;
    }
    match &config.tool_choice {
        AiToolChoice::Auto => {}
        AiToolChoice::Required => {
            body.insert(
                "tool_choice".to_string(),
                serde_json::json!({ "type": "any" }),
            );
        }
        AiToolChoice::Named(name) if !name.is_empty() => {
            body.insert(
                "tool_choice".to_string(),
                serde_json::json!({ "type": "tool", "name": name }),
            );
        }
        AiToolChoice::Named(_) => {}
    }
}

fn anthropic_tool_definitions(tools: &[AiToolDefinition]) -> Vec<Value> {
    tools
        .iter()
        .map(|tool| {
            serde_json::json!({
                "name": tool.name,
                "description": tool.description,
                "input_schema": tool.parameters,
            })
        })
        .collect()
}

fn anthropic_thinking_budget(config: &AiChatStreamConfig) -> Option<i64> {
    let effort = config.reasoning_effort.as_deref().unwrap_or("auto");
    if matches!(effort, "auto" | "off" | "none") || config.provider_type != "anthropic" {
        return None;
    }

    let desired = match effort {
        "max" | "xhigh" => 8192,
        "high" => 4096,
        "medium" => 2048,
        _ => 1024,
    };
    let max_tokens = config.max_response_tokens.unwrap_or(8192).max(1);
    let capped = desired.min((max_tokens - 1024).max(0));
    (capped >= 1024).then_some(capped)
}

pub(crate) fn anthropic_chat_messages(messages: &[AiChatMessage]) -> (Option<String>, Vec<Value>) {
    let mut system_parts = Vec::new();
    let mut converted = Vec::<(String, Value)>::new();
    for message in messages {
        match message.role {
            AiChatRole::System if !message.content.is_empty() => {
                system_parts.push(message.content.clone());
            }
            AiChatRole::System => {}
            AiChatRole::User => {
                converted.push(("user".to_string(), Value::String(message.content.clone())))
            }
            AiChatRole::Tool => converted.push((
                "user".to_string(),
                serde_json::json!([{
                    "type": "tool_result",
                    "tool_use_id": message.tool_call_id.as_deref().unwrap_or_default(),
                    "content": message.content,
                }]),
            )),
            AiChatRole::Assistant => {
                let calls = message
                    .tool_calls
                    .iter()
                    .filter_map(AiToolCall::from_value)
                    .collect::<Vec<_>>();
                if calls.is_empty() {
                    converted.push((
                        "assistant".to_string(),
                        Value::String(message.content.clone()),
                    ));
                } else {
                    let mut parts = Vec::new();
                    if !message.content.is_empty() {
                        parts.push(serde_json::json!({ "type": "text", "text": message.content }));
                    }
                    parts.extend(calls.into_iter().map(|call| {
                        let input = serde_json::from_str::<Value>(&call.arguments)
                            .unwrap_or_else(|_| serde_json::json!({}));
                        serde_json::json!({
                            "type": "tool_use",
                            "id": call.id,
                            "name": call.name,
                            "input": input,
                        })
                    }));
                    converted.push(("assistant".to_string(), Value::Array(parts)));
                }
            }
        }
    }

    let mut merged = Vec::<(String, Value)>::new();
    for (role, content) in converted {
        if let Some((last_role, last_content)) = merged.last_mut()
            && *last_role == role
        {
            if let (Some(last_text), Some(next_text)) = (last_content.as_str(), content.as_str()) {
                *last_content = Value::String(format!("{last_text}\n\n{next_text}"));
            } else {
                let mut parts = anthropic_content_as_array(last_content);
                parts.extend(anthropic_content_as_array(&content));
                *last_content = Value::Array(parts);
            }
            continue;
        }
        merged.push((role, content));
    }
    if merged.first().is_some_and(|(role, _)| role != "user") {
        merged.insert(
            0,
            (
                "user".to_string(),
                Value::String("(Continue from previous context)".to_string()),
            ),
        );
    }

    let messages = merged
        .into_iter()
        .map(|(role, content)| serde_json::json!({ "role": role, "content": content }))
        .collect::<Vec<_>>();
    let system = (!system_parts.is_empty()).then(|| system_parts.join("\n\n"));
    (system, messages)
}

fn anthropic_content_as_array(content: &Value) -> Vec<Value> {
    if let Some(array) = content.as_array() {
        return array.clone();
    }
    vec![serde_json::json!({
        "type": "text",
        "text": content.as_str().unwrap_or_default(),
    })]
}

#[cfg(test)]
pub(crate) fn parse_anthropic_data_line(line: &str) -> ParsedStreamLine {
    let mut accumulator = AnthropicToolAccumulator::default();
    parse_anthropic_data_line_with_accumulator(line, &mut accumulator)
}

#[derive(Default)]
pub(crate) struct AnthropicToolAccumulator {
    active: Option<AnthropicToolCallChunk>,
}

#[derive(Default)]
struct AnthropicToolCallChunk {
    id: String,
    name: String,
    arguments: String,
}

pub(crate) fn parse_anthropic_data_line_with_accumulator(
    line: &str,
    accumulator: &mut AnthropicToolAccumulator,
) -> ParsedStreamLine {
    let Some(data) = line.strip_prefix("data: ") else {
        return ParsedStreamLine {
            events: Vec::new(),
            saw_frame: false,
        };
    };
    let data = data.trim();
    if data.is_empty() {
        return ParsedStreamLine {
            events: Vec::new(),
            saw_frame: true,
        };
    }

    let mut events = Vec::new();
    if let Ok(json) = serde_json::from_str::<Value>(data) {
        match json.get("type").and_then(Value::as_str) {
            Some("content_block_start") => {
                if json
                    .get("content_block")
                    .and_then(|block| block.get("type"))
                    .and_then(Value::as_str)
                    == Some("tool_use")
                {
                    let block = json.get("content_block").unwrap_or(&Value::Null);
                    let id = block
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let name = block
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    accumulator.active = Some(AnthropicToolCallChunk {
                        id,
                        name,
                        arguments: String::new(),
                    });
                }
            }
            Some("content_block_delta") => {
                let delta = json.get("delta");
                match delta
                    .and_then(|delta| delta.get("type"))
                    .and_then(Value::as_str)
                {
                    Some("thinking_delta") => {
                        if let Some(thinking) = delta
                            .and_then(|delta| delta.get("thinking"))
                            .and_then(Value::as_str)
                            .filter(|thinking| !thinking.is_empty())
                        {
                            events.push(AiStreamEvent::Thinking(thinking.to_string()));
                        }
                    }
                    Some("text_delta") => {
                        if let Some(text) = delta
                            .and_then(|delta| delta.get("text"))
                            .and_then(Value::as_str)
                            .filter(|text| !text.is_empty())
                        {
                            events.push(AiStreamEvent::Content(text.to_string()));
                        }
                    }
                    Some("input_json_delta") => {
                        if let Some(partial) = delta
                            .and_then(|delta| delta.get("partial_json"))
                            .and_then(Value::as_str)
                            .filter(|partial| !partial.is_empty())
                            && let Some(active) = accumulator.active.as_mut()
                        {
                            active.arguments.push_str(partial);
                            events.push(AiStreamEvent::ToolCall {
                                id: active.id.clone(),
                                name: active.name.clone(),
                                arguments: active.arguments.clone(),
                            });
                        }
                    }
                    _ => {}
                }
            }
            Some("content_block_stop") => {
                if let Some(active) = accumulator.active.take() {
                    events.push(AiStreamEvent::ToolCallComplete {
                        id: active.id,
                        name: active.name,
                        arguments: active.arguments,
                    });
                }
            }
            Some("message_stop") => events.push(AiStreamEvent::Done),
            Some("error") => {
                let message = json
                    .get("error")
                    .and_then(|error| error.get("message"))
                    .and_then(Value::as_str)
                    .unwrap_or("Anthropic stream error");
                events.push(AiStreamEvent::Error(message.to_string()));
            }
            _ => {}
        }
    }
    ParsedStreamLine {
        events,
        saw_frame: true,
    }
}

fn parse_anthropic_error(status: u16, body: &str) -> String {
    let mut fallback = format!("Anthropic API error: {status}");
    if let Ok(json) = serde_json::from_str::<Value>(body) {
        if let Some(message) = json
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(Value::as_str)
        {
            return message.to_string();
        }
    } else if !body.is_empty() {
        fallback = body.chars().take(200).collect();
    }
    fallback
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AiPolicySafetyMode, AiToolUsePolicy};

    fn config(reasoning_effort: &str, max_response_tokens: i64) -> AiChatStreamConfig {
        AiChatStreamConfig {
            provider_id: Some("anthropic".to_string()),
            provider_type: "anthropic".to_string(),
            base_url: "https://api.anthropic.com".to_string(),
            model: "claude".to_string(),
            api_key: None,
            max_response_tokens: Some(max_response_tokens),
            reasoning_effort: Some(reasoning_effort.to_string()),
            safety_mode: AiPolicySafetyMode::Default,
            profile_id: None,
            tool_policy: AiToolUsePolicy::default(),
            tools: Vec::new(),
            tool_choice: AiToolChoice::Auto,
        }
    }

    #[test]
    fn anthropic_thinking_budget_matches_tauri_cap() {
        let message = AiChatMessage {
            id: "u1".to_string(),
            role: AiChatRole::User,
            content: "hello".to_string(),
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
        };
        let body = anthropic_chat_body(&config("high", 4096), &[message]);
        assert_eq!(body["thinking"]["type"].as_str(), Some("enabled"));
        assert_eq!(body["thinking"]["budget_tokens"].as_i64(), Some(3072));

        let body = anthropic_chat_body(&config("off", 4096), &[]);
        assert!(body.get("thinking").is_none());
    }

    #[test]
    fn anthropic_tool_payload_matches_tauri_shape() {
        let mut tool_config = config("off", 4096);
        tool_config.tools = vec![AiToolDefinition {
            name: "get_state".to_string(),
            description: "Get state".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": { "scope": { "type": "string" } },
                "required": ["scope"],
            }),
        }];
        tool_config.tool_choice = AiToolChoice::Required;
        let body = anthropic_chat_body(&tool_config, &[]);
        assert_eq!(body["tools"][0]["name"].as_str(), Some("get_state"));
        assert_eq!(
            body["tools"][0]["input_schema"]["type"].as_str(),
            Some("object")
        );
        assert_eq!(body["tool_choice"]["type"].as_str(), Some("any"));

        let mut thinking_config = config("high", 4096);
        thinking_config.tools = tool_config.tools;
        thinking_config.tool_choice = AiToolChoice::Required;
        let body = anthropic_chat_body(&thinking_config, &[]);
        assert!(body.get("tools").is_some());
        assert!(body.get("tool_choice").is_none());
    }

    #[test]
    fn anthropic_message_conversion_preserves_tool_use_and_results() {
        let assistant = AiChatMessage {
            id: "a1".to_string(),
            role: AiChatRole::Assistant,
            content: "I'll check.".to_string(),
            timestamp_ms: 1,
            model: None,
            context: None,
            thinking_content: None,
            is_streaming: false,
            metadata: None,
            tool_call_id: None,
            tool_calls: vec![serde_json::json!({
                "id": "call-1",
                "name": "get_state",
                "arguments": "{\"scope\":\"active\"}",
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

        let (_, converted) = anthropic_chat_messages(&[assistant, result]);
        assert_eq!(converted[0]["role"].as_str(), Some("user"));
        assert_eq!(converted[1]["role"].as_str(), Some("assistant"));
        assert_eq!(
            converted[1]["content"][1]["type"].as_str(),
            Some("tool_use")
        );
        assert_eq!(
            converted[1]["content"][1]["input"]["scope"].as_str(),
            Some("active")
        );
        assert_eq!(
            converted[2]["content"][0]["type"].as_str(),
            Some("tool_result")
        );
        assert_eq!(
            converted[2]["content"][0]["tool_use_id"].as_str(),
            Some("call-1")
        );
    }

    #[test]
    fn anthropic_invalid_tool_arguments_fall_back_to_empty_object_like_tauri() {
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
                "name": "get_state",
                "arguments": "{not json",
            })],
            turn: None,
            transcript_ref: None,
            summary_ref: None,
            branches: None,
            suggestions: Vec::new(),
        };

        let (_, converted) = anthropic_chat_messages(&[assistant]);

        assert_eq!(converted[1]["content"][0]["input"], serde_json::json!({}));
    }

    #[test]
    fn anthropic_stream_parser_assembles_tool_use_chunks() {
        let mut accumulator = AnthropicToolAccumulator::default();
        let start = parse_anthropic_data_line_with_accumulator(
            r#"data: {"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"call-1","name":"get_state","input":{}}}"#,
            &mut accumulator,
        );
        assert!(start.events.is_empty());
        let delta = parse_anthropic_data_line_with_accumulator(
            r#"data: {"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"scope\":\"active\"}"}}"#,
            &mut accumulator,
        );
        assert_eq!(
            delta.events,
            vec![AiStreamEvent::ToolCall {
                id: "call-1".to_string(),
                name: "get_state".to_string(),
                arguments: "{\"scope\":\"active\"}".to_string(),
            }]
        );
        let stop = parse_anthropic_data_line_with_accumulator(
            r#"data: {"type":"content_block_stop","index":1}"#,
            &mut accumulator,
        );
        assert_eq!(
            stop.events,
            vec![AiStreamEvent::ToolCallComplete {
                id: "call-1".to_string(),
                name: "get_state".to_string(),
                arguments: "{\"scope\":\"active\"}".to_string(),
            }]
        );
    }

    #[test]
    fn anthropic_stream_parser_requires_delta_type_like_tauri() {
        let mut accumulator = AnthropicToolAccumulator::default();
        let parsed = parse_anthropic_data_line_with_accumulator(
            r#"data: {"type":"content_block_delta","index":0,"delta":{"text":"ignored","thinking":"ignored","partial_json":"{}"}}"#,
            &mut accumulator,
        );
        assert!(parsed.events.is_empty());
    }
}
