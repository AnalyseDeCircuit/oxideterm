use anyhow::{Context, Result, anyhow};
use serde_json::Value;

use crate::providers::{ANTHROPIC_VERSION, api_key_required_ref};
use crate::{
    AiChatMessage, AiChatRole, AiChatStreamConfig, AiReasoningLevel, AiReasoningRequestFormat,
    AiStreamEvent, AiToolCall, AiToolChoice, AiToolDefinition, model_reasoning_capability,
};

use super::CHAT_STREAM_TIMEOUT;
use super::common::{ParsedStreamLine, StreamParseResult, stream_sse_response};

pub(crate) async fn stream_anthropic_completion(
    config: AiChatStreamConfig,
    messages: Vec<AiChatMessage>,
    events: tokio::sync::mpsc::UnboundedSender<AiStreamEvent>,
) -> Result<()> {
    let api_key = api_key_required_ref(
        &config.provider_type,
        config.api_key.as_ref().map(|api_key| api_key.as_str()),
    )?;
    let url = format!(
        "{}/v1/messages",
        config.base_url.trim().trim_end_matches('/')
    );
    // Reuse the application pool while keeping the API key on this request only.
    let client = oxideterm_network_proxy::application_http_client()
        .context("failed to acquire application Anthropic chat client")?;
    let body = anthropic_chat_body(&config, &messages);
    let response = client
        .post(&url)
        .timeout(CHAT_STREAM_TIMEOUT)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&body)
        .send()
        .await
        .map_err(|error| anyhow::Error::new(error.without_url()))?;
    if !response.status().is_success() {
        super::retry::check_transient_response(&response)?;
        let status = response.status().as_u16();
        let error_text = response.text().await.unwrap_or_default();
        return Err(anyhow!(parse_anthropic_error(status, &error_text)));
    }
    let mut accumulator = AnthropicToolAccumulator::default();
    let result = stream_sse_response(response, &events, |line| {
        parse_anthropic_data_line_with_accumulator(line, &mut accumulator)
    })
    .await?;
    if !matches!(result, StreamParseResult::Done) {
        return Err(anyhow!("ai_stream_interrupted"));
    }
    let _ = events.send(AiStreamEvent::Done);
    Ok(())
}

fn anthropic_chat_body(config: &AiChatStreamConfig, messages: &[AiChatMessage]) -> Value {
    let (system, mut api_messages) = anthropic_chat_messages(messages);
    apply_anthropic_message_cache_breakpoints(&mut api_messages);
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
    apply_anthropic_effort(&mut body, config);
    if let Some(system) = system.filter(|system| !system.is_empty())
        && let Some(object) = body.as_object_mut()
    {
        // Anthropic requires explicit ephemeral cache breakpoints; structured
        // text blocks let the static system prefix hit cache across turns.
        object.insert(
            "system".to_string(),
            serde_json::json!([{
                "type": "text",
                "text": system,
                "cache_control": { "type": "ephemeral" },
            }]),
        );
    }
    if let Some(object) = body.as_object_mut() {
        let thinking_enabled = object.get("thinking").is_some();
        apply_anthropic_tool_options(object, config, thinking_enabled);
    }
    body
}

fn apply_anthropic_message_cache_breakpoints(messages: &mut [Value]) {
    // Anthropic allows at most 4 cache_control blocks per request:
    // 1 on tools, 1 on system, and up to 2 on messages (the previous completed
    // turn boundary and the latest message for multi-round tool loops).
    let len = messages.len();
    if len >= 2 {
        mark_anthropic_message_cacheable(&mut messages[len - 2]);
    }
    if len >= 1 {
        mark_anthropic_message_cacheable(&mut messages[len - 1]);
    }
}

fn mark_anthropic_message_cacheable(message: &mut Value) {
    let Some(content) = message.get_mut("content") else {
        return;
    };
    match content {
        Value::String(text) if !text.is_empty() => {
            let owned = std::mem::take(text);
            *content = serde_json::json!([{
                "type": "text",
                "text": owned,
                "cache_control": { "type": "ephemeral" },
            }]);
        }
        Value::Array(blocks) => {
            if let Some(block) = blocks.iter_mut().rev().find(|block| {
                let Some(object) = block.as_object() else {
                    return false;
                };
                if object.get("type").and_then(Value::as_str) == Some("text") {
                    object
                        .get("text")
                        .and_then(Value::as_str)
                        .is_some_and(|text| !text.is_empty())
                } else {
                    true
                }
            }) && let Some(object) = block.as_object_mut()
            {
                object.insert(
                    "cache_control".to_string(),
                    serde_json::json!({ "type": "ephemeral" }),
                );
            }
        }
        _ => {}
    }
}

fn apply_anthropic_effort(body: &mut Value, config: &AiChatStreamConfig) {
    let effort = AiReasoningLevel::parse(config.reasoning_effort.as_deref().unwrap_or("auto"));
    let capability = model_reasoning_capability(&config.provider_type, &config.model);
    if effort == AiReasoningLevel::Auto
        || capability.request_format != AiReasoningRequestFormat::AnthropicEffort
    {
        return;
    }
    if let Some(object) = body.as_object_mut() {
        object.insert(
            "output_config".to_string(),
            serde_json::json!({ "effort": effort.as_str() }),
        );
        // Current effort-capable Claude models use adaptive thinking rather
        // than the deprecated manual budget_tokens control.
        if !matches!(effort, AiReasoningLevel::None) {
            object.insert(
                "thinking".to_string(),
                serde_json::json!({ "type": "adaptive" }),
            );
        }
    }
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
    let mut definitions = tools
        .iter()
        .map(|tool| {
            serde_json::json!({
                "name": tool.name,
                "description": tool.description,
                "input_schema": tool.parameters,
            })
        })
        .collect::<Vec<_>>();
    if let Some(last) = definitions.last_mut().and_then(Value::as_object_mut) {
        last.insert(
            "cache_control".to_string(),
            serde_json::json!({ "type": "ephemeral" }),
        );
    }
    definitions
}

fn anthropic_thinking_budget(config: &AiChatStreamConfig) -> Option<i64> {
    if model_reasoning_capability(&config.provider_type, &config.model).request_format
        == AiReasoningRequestFormat::AnthropicEffort
    {
        return None;
    }
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
    let mut system: Option<String> = None;
    let mut converted = Vec::<(String, Value)>::new();
    for message in messages {
        if crate::runtime_context::is_user_context_system_message(message) {
            converted.push(("user".to_string(), Value::String(message.content.clone())));
            continue;
        }
        match message.role {
            AiChatRole::System => {
                system = Some(match system {
                    // Match Tauri's JS truthiness when folding system prompts:
                    // an empty accumulated prompt is replaced, while a
                    // non-empty prompt keeps separators even for empty input.
                    Some(current) if !current.is_empty() => {
                        format!("{current}\n\n{}", message.content)
                    }
                    _ => message.content.clone(),
                });
            }
            AiChatRole::User => converted.push((
                "user".to_string(),
                if crate::message_images(message).is_empty() {
                    Value::String(message.content.clone())
                } else {
                    crate::images::image_message_content(message, "anthropic")
                },
            )),
            AiChatRole::Tool => {
                let mut tool_result = serde_json::json!({
                    "type": "tool_result",
                    "content": if crate::message_images(message).is_empty() { Value::String(message.content.clone()) } else { crate::images::image_message_content(message, "anthropic") },
                });
                if let Some(tool_call_id) = message.tool_call_id.as_ref()
                    && let Some(object) = tool_result.as_object_mut()
                {
                    object.insert(
                        "tool_use_id".to_string(),
                        Value::String(tool_call_id.clone()),
                    );
                }
                converted.push(("user".to_string(), Value::Array(vec![tool_result])));
            }
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
        if let Some(usage) = json
            .get("usage")
            .or_else(|| json.get("message").and_then(|message| message.get("usage")))
        {
            let raw_input = usage.get("input_tokens").and_then(Value::as_u64);
            let cache_read = usage.get("cache_read_input_tokens").and_then(Value::as_u64);
            let cache_write = usage
                .get("cache_creation_input_tokens")
                .and_then(Value::as_u64);
            // Anthropic reports non-cached, cache-read, and cache-creation input tokens
            // separately; sum them so input_tokens matches total prompt tokens across providers.
            let input_tokens = match (raw_input, cache_read, cache_write) {
                (None, None, None) => None,
                (raw, read, write) => Some(
                    raw.unwrap_or(0)
                        .saturating_add(read.unwrap_or(0))
                        .saturating_add(write.unwrap_or(0)),
                ),
            };
            events.push(AiStreamEvent::Usage {
                input_tokens,
                output_tokens: usage.get("output_tokens").and_then(Value::as_u64),
                cache_read_tokens: cache_read,
                cache_write_tokens: cache_write,
            });
        }
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
            Some("message_delta")
                if json.pointer("/delta/stop_reason").and_then(Value::as_str)
                    == Some("max_tokens") =>
            {
                events.push(AiStreamEvent::Error("ai_output_incomplete".into()));
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
    use crate::{AiExecutionBackend, AiPolicySafetyMode, AiToolUsePolicy};

    #[test]
    fn image_input_anthropic_keeps_an_image_only_user_turn() {
        let mut message: AiChatMessage = serde_json::from_value(
            serde_json::json!({"id":"image","role":"user","content":"","timestamp_ms":1}),
        )
        .unwrap();
        crate::set_message_images(
            &mut message,
            vec![
                crate::AiImageAttachment::new(
                    "screenshot.png".into(),
                    "image/png".into(),
                    200,
                    100,
                    b"pixel",
                    b"thumb",
                )
                .unwrap(),
            ],
        );
        let body = anthropic_chat_body(&config("auto", 4096), &[message.clone()]);
        assert_eq!(
            body["messages"][0],
            serde_json::json!({"role":"user", "content":[
                {"type":"image","source":{"type":"base64","media_type":"image/png","data":"cGl4ZWw="},"cache_control":{"type":"ephemeral"}}
            ]})
        );
        message.role = AiChatRole::Tool;
        message.tool_call_id = Some("screen-call".into());
        message.content = "Captured".into();
        let body = anthropic_chat_body(&config("auto", 4096), &[message]);
        assert_eq!(
            body["messages"][0]["content"][0],
            serde_json::json!({
                "type":"tool_result","tool_use_id":"screen-call","content":[
                    {"type":"text","text":"Captured"},
                    {"type":"image","source":{"type":"base64","media_type":"image/png","data":"cGl4ZWw="}}
                ],"cache_control":{"type":"ephemeral"}
            })
        );
    }

    fn config(reasoning_effort: &str, max_response_tokens: i64) -> AiChatStreamConfig {
        AiChatStreamConfig {
            api_protocol: crate::AiApiProtocol::default(),
            image_input: Default::default(),
            omit_history_images: false,
            execution_backend: AiExecutionBackend::Provider,
            provider_id: Some("anthropic".to_string()),
            acp_agent_id: None,
            acp_session_id: None,
            acp_config_selection: None,
            provider_type: "anthropic".to_string(),
            base_url: "https://api.anthropic.com".to_string(),
            model: "claude-sonnet-4-6".to_string(),
            api_key: None,
            max_response_tokens: Some(max_response_tokens),
            reasoning_effort: Some(reasoning_effort.to_string()),
            safety_mode: AiPolicySafetyMode::Default,
            profile_id: None,
            memory_context: None,
            memory_entry_ids: Vec::new(),
            prompt_cache_key: None,
            tool_policy: AiToolUsePolicy::default(),
            tools: Vec::new(),
            tool_choice: AiToolChoice::Auto,
        }
    }

    #[test]
    fn anthropic_effort_uses_current_output_config_and_adaptive_thinking() {
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
        assert_eq!(body["thinking"]["type"].as_str(), Some("adaptive"));
        assert_eq!(body["output_config"]["effort"].as_str(), Some("high"));
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
    fn anthropic_prompt_caching_places_bounded_breakpoints_and_normalizes_usage() {
        let mut cfg = config("auto", 4096);
        cfg.tools.push(AiToolDefinition {
            name: "list_targets".to_string(),
            description: "List targets".to_string(),
            parameters: serde_json::json!({"type": "object"}),
        });
        let messages: Vec<AiChatMessage> = serde_json::from_value(serde_json::json!([
            {"id":"base-system","role":"system","content":"Static rules","timestamp_ms":0},
            {"id":"u1","role":"user","content":"First question","timestamp_ms":1},
            {"id":"a1","role":"assistant","content":"First answer","timestamp_ms":2},
            {"id":"u2","role":"user","content":"Second question","timestamp_ms":3},
            {"id":"task-mode","role":"system","content":"Volatile env","timestamp_ms":4}
        ]))
        .unwrap();
        let body = anthropic_chat_body(&cfg, &messages);
        assert_eq!(
            body["system"],
            serde_json::json!([{"type":"text","text":"Static rules","cache_control":{"type":"ephemeral"}}])
        );
        assert_eq!(
            body["tools"][0]["cache_control"],
            serde_json::json!({"type":"ephemeral"})
        );
        assert_eq!(
            body["messages"][1]["content"][0]["cache_control"],
            serde_json::json!({"type":"ephemeral"})
        );
        assert_eq!(
            body["messages"][2]["content"][0]["cache_control"],
            serde_json::json!({"type":"ephemeral"})
        );
        assert!(body["messages"][0]["content"].is_string());

        let parsed = parse_anthropic_data_line(
            r#"data: {"type":"message_start","message":{"usage":{"input_tokens":200,"cache_read_input_tokens":1800,"cache_creation_input_tokens":400,"output_tokens":1}}}"#,
        );
        assert_eq!(
            parsed.events,
            vec![AiStreamEvent::Usage {
                input_tokens: Some(2400),
                output_tokens: Some(1),
                cache_read_tokens: Some(1800),
                cache_write_tokens: Some(400),
            }]
        );
    }
}
