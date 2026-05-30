use anyhow::{Context, Result, anyhow};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::providers::{api_key_required_ref, url_encode_component};
use crate::{
    AiChatMessage, AiChatRole, AiChatStreamConfig, AiStreamEvent, AiToolCall, AiToolChoice,
    AiToolDefinition,
};

use super::CHAT_STREAM_TIMEOUT;
use super::common::{ParsedStreamLine, stream_sse_response};

static GEMINI_TOOL_CALL_COUNTER: AtomicU64 = AtomicU64::new(1);

pub(crate) async fn stream_gemini_completion(
    config: AiChatStreamConfig,
    messages: Vec<AiChatMessage>,
    events: tokio::sync::mpsc::UnboundedSender<AiStreamEvent>,
) -> Result<()> {
    let api_key = api_key_required_ref(&config.provider_type, config.api_key.as_ref())?;
    let url = format!(
        "{}/models/{}:streamGenerateContent",
        config.base_url.trim().trim_end_matches('/'),
        url_encode_component(&config.model)
    );
    let client = reqwest::Client::builder()
        .timeout(CHAT_STREAM_TIMEOUT)
        .build()
        .context("failed to create Gemini chat client")?;
    let body = gemini_chat_body(&config, &messages);
    let response = client
        .post(&url)
        // Gemini requires the API key as a query parameter. Let reqwest attach
        // it to the request and strip URLs from transport errors below.
        .query(&[("alt", "sse"), ("key", api_key.as_str())])
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|error| {
            anyhow!(
                "failed to connect to Gemini provider: {}",
                error.without_url()
            )
        })?;
    if !response.status().is_success() {
        let status = response.status().as_u16();
        let error_text = response.text().await.unwrap_or_default();
        return Err(anyhow!(parse_gemini_error(status, &error_text)));
    }
    let _ = stream_sse_response(response, &events, parse_gemini_data_line).await?;
    let _ = events.send(AiStreamEvent::Done);
    Ok(())
}

pub(crate) fn gemini_chat_body(config: &AiChatStreamConfig, messages: &[AiChatMessage]) -> Value {
    let (system_instruction, contents) = gemini_chat_contents(messages);
    let mut body = serde_json::json!({ "contents": contents });
    if let Some(system) = system_instruction.filter(|system| !system.is_empty())
        && let Some(object) = body.as_object_mut()
    {
        object.insert(
            "system_instruction".to_string(),
            serde_json::json!({ "parts": [{ "text": system }] }),
        );
    }
    if let Some(tokens) = config.max_response_tokens.filter(|tokens| *tokens > 0)
        && let Some(object) = body.as_object_mut()
    {
        object.insert(
            "generationConfig".to_string(),
            serde_json::json!({ "maxOutputTokens": tokens }),
        );
    }
    if !config.tools.is_empty()
        && let Some(object) = body.as_object_mut()
    {
        object.insert(
            "tools".to_string(),
            serde_json::json!(gemini_tool_definitions(&config.tools)),
        );
        if let Some(tool_config) = gemini_tool_config(&config.tool_choice) {
            object.insert("toolConfig".to_string(), tool_config);
        }
    }
    body
}

pub(crate) fn gemini_chat_contents(messages: &[AiChatMessage]) -> (Option<String>, Vec<Value>) {
    let mut system_instruction: Option<String> = None;
    let mut contents = Vec::<Value>::new();
    let mut tool_names_by_id = HashMap::<String, String>::new();
    for message in messages {
        match message.role {
            AiChatRole::System => {
                system_instruction = Some(match system_instruction {
                    // Gemini's Tauri adapter uses JavaScript truthiness here:
                    // an empty previous system prompt is replaced, while a
                    // non-empty one keeps the "\n\n" separator even for empty
                    // later system prompts.
                    Some(current) if !current.is_empty() => {
                        format!("{current}\n\n{}", message.content)
                    }
                    _ => message.content.clone(),
                });
            }
            AiChatRole::Tool => {
                let name = message
                    .tool_call_id
                    .as_ref()
                    .and_then(|id| tool_names_by_id.get(id))
                    .cloned()
                    .unwrap_or_else(|| "unknown".to_string());
                let response = serde_json::from_str::<Value>(&message.content)
                    .unwrap_or_else(|_| serde_json::json!({ "output": message.content }));
                contents.push(serde_json::json!({
                    "role": "user",
                    "parts": [{ "functionResponse": { "name": name, "response": response } }],
                }));
            }
            AiChatRole::Assistant if !message.tool_calls.is_empty() => {
                let mut parts = Vec::new();
                if !message.content.is_empty() {
                    parts.push(serde_json::json!({ "text": message.content }));
                }
                for call in message.tool_calls.iter().filter_map(AiToolCall::from_value) {
                    tool_names_by_id.insert(call.id.clone(), call.name.clone());
                    // Tauri passes any successfully parsed JSON value through
                    // to Gemini functionCall.args; only parse failures become
                    // an empty object.
                    let args = serde_json::from_str::<Value>(&call.arguments)
                        .unwrap_or_else(|_| serde_json::json!({}));
                    parts.push(serde_json::json!({
                        "functionCall": { "name": call.name, "args": args },
                    }));
                }
                contents.push(serde_json::json!({ "role": "model", "parts": parts }));
            }
            AiChatRole::User | AiChatRole::Assistant => {
                let role = if message.role == AiChatRole::Assistant {
                    "model"
                } else {
                    "user"
                };
                if let Some(last) = contents.last_mut()
                    && last.get("role").and_then(Value::as_str) == Some(role)
                    && let Some(parts) = last.get_mut("parts").and_then(Value::as_array_mut)
                {
                    parts.push(serde_json::json!({ "text": message.content }));
                    continue;
                }
                contents.push(serde_json::json!({
                    "role": role,
                    "parts": [{ "text": message.content }],
                }));
            }
        }
    }
    if contents
        .first()
        .is_some_and(|content| content.get("role").and_then(Value::as_str) != Some("user"))
    {
        contents.insert(
            0,
            serde_json::json!({ "role": "user", "parts": [{ "text": "(Continue)" }] }),
        );
    }
    (system_instruction, contents)
}

fn gemini_tool_definitions(tools: &[AiToolDefinition]) -> Vec<Value> {
    vec![serde_json::json!({
        "functionDeclarations": tools
            .iter()
            .map(|tool| serde_json::json!({
                "name": tool.name,
                "description": tool.description,
                "parameters": tool.parameters,
            }))
            .collect::<Vec<_>>(),
    })]
}

fn gemini_tool_config(tool_choice: &AiToolChoice) -> Option<Value> {
    match tool_choice {
        AiToolChoice::Auto => None,
        AiToolChoice::Required => Some(serde_json::json!({
            "functionCallingConfig": { "mode": "ANY" },
        })),
        AiToolChoice::Named(name) if !name.is_empty() => Some(serde_json::json!({
            "functionCallingConfig": {
                "mode": "ANY",
                "allowedFunctionNames": [name],
            },
        })),
        AiToolChoice::Named(_) => None,
    }
}

pub(crate) fn parse_gemini_data_line(line: &str) -> ParsedStreamLine {
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
    if let Ok(json) = serde_json::from_str::<Value>(data)
        && let Some(parts) = json
            .get("candidates")
            .and_then(Value::as_array)
            .and_then(|candidates| candidates.first())
            .and_then(|candidate| candidate.get("content"))
            .and_then(|content| content.get("parts"))
            .and_then(Value::as_array)
    {
        for part in parts {
            if let Some(text) = part
                .get("text")
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty())
            {
                events.push(AiStreamEvent::Content(text.to_string()));
            }
            if let Some(function_call) = part.get("functionCall") {
                let id = format!(
                    "gemini-{}",
                    GEMINI_TOOL_CALL_COUNTER.fetch_add(1, Ordering::Relaxed)
                );
                let name = function_call
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();
                let arguments = function_call
                    .get("args")
                    .filter(|args| gemini_js_truthy(args))
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({}))
                    .to_string();
                events.push(AiStreamEvent::ToolCallComplete {
                    id,
                    name,
                    arguments,
                });
            }
        }
    }
    ParsedStreamLine {
        events,
        saw_frame: true,
    }
}

fn gemini_js_truthy(value: &Value) -> bool {
    // Tauri uses `part.functionCall.args || {}` before JSON.stringify. Mirror
    // JavaScript truthiness for JSON values so arrays/objects are preserved
    // while null, false, zero, and empty string fall back to `{}`.
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64() != Some(0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

fn parse_gemini_error(status: u16, body: &str) -> String {
    let mut fallback = format!("Gemini API error: {status}");
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
