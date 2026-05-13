use anyhow::{Context, Result, anyhow};
use serde_json::Value;

use crate::providers::{ANTHROPIC_VERSION, api_key_required_ref};
use crate::{AiChatMessage, AiChatRole, AiChatStreamConfig, AiStreamEvent};

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
        .with_context(|| format!("failed to connect to Anthropic provider at {url}"))?;
    if !response.status().is_success() {
        let status = response.status().as_u16();
        let error_text = response.text().await.unwrap_or_default();
        return Err(anyhow!(parse_anthropic_error(status, &error_text)));
    }
    let _ = stream_sse_response(response, &events, parse_anthropic_data_line).await?;
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
    if let Some(system) = system
        && let Some(object) = body.as_object_mut()
    {
        object.insert("system".to_string(), serde_json::json!(system));
    }
    body
}

pub(crate) fn anthropic_chat_messages(messages: &[AiChatMessage]) -> (Option<String>, Vec<Value>) {
    let mut system_parts = Vec::new();
    let mut converted = Vec::<(String, String)>::new();
    for message in messages {
        match message.role {
            AiChatRole::System if !message.content.is_empty() => {
                system_parts.push(message.content.clone());
            }
            AiChatRole::System => {}
            AiChatRole::User => converted.push(("user".to_string(), message.content.clone())),
            AiChatRole::Assistant => {
                converted.push(("assistant".to_string(), message.content.clone()))
            }
        }
    }

    let mut merged = Vec::<(String, String)>::new();
    for (role, content) in converted {
        if let Some((last_role, last_content)) = merged.last_mut()
            && *last_role == role
        {
            if !content.is_empty() {
                if !last_content.is_empty() {
                    last_content.push_str("\n\n");
                }
                last_content.push_str(&content);
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
                "(Continue from previous context)".to_string(),
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

pub(crate) fn parse_anthropic_data_line(line: &str) -> ParsedStreamLine {
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
            Some("content_block_delta") => {
                let delta = json.get("delta");
                if let Some(text) = delta
                    .and_then(|delta| delta.get("text"))
                    .and_then(Value::as_str)
                    .filter(|text| !text.is_empty())
                {
                    events.push(AiStreamEvent::Content(text.to_string()));
                }
                if let Some(thinking) = delta
                    .and_then(|delta| delta.get("thinking"))
                    .and_then(Value::as_str)
                    .filter(|thinking| !thinking.is_empty())
                {
                    events.push(AiStreamEvent::Thinking(thinking.to_string()));
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
