use anyhow::{Context, Result, anyhow};
use serde_json::Value;

use crate::providers::{api_key_required_ref, url_encode_component};
use crate::{AiChatMessage, AiChatRole, AiChatStreamConfig, AiStreamEvent};

use super::CHAT_STREAM_TIMEOUT;
use super::common::{ParsedStreamLine, stream_sse_response};

pub(crate) async fn stream_gemini_completion(
    config: AiChatStreamConfig,
    messages: Vec<AiChatMessage>,
    events: tokio::sync::mpsc::UnboundedSender<AiStreamEvent>,
) -> Result<()> {
    let api_key = api_key_required_ref(&config.provider_type, config.api_key.as_ref())?;
    let url = format!(
        "{}/models/{}:streamGenerateContent?alt=sse&key={}",
        config.base_url.trim().trim_end_matches('/'),
        url_encode_component(&config.model),
        url_encode_component(api_key.as_str())
    );
    let client = reqwest::Client::builder()
        .timeout(CHAT_STREAM_TIMEOUT)
        .build()
        .context("failed to create Gemini chat client")?;
    let body = gemini_chat_body(&config, &messages);
    let response = client
        .post(&url)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .json(&body)
        .send()
        .await
        .context("failed to connect to Gemini provider")?;
    if !response.status().is_success() {
        let status = response.status().as_u16();
        let error_text = response.text().await.unwrap_or_default();
        return Err(anyhow!(parse_gemini_error(status, &error_text)));
    }
    let _ = stream_sse_response(response, &events, parse_gemini_data_line).await?;
    let _ = events.send(AiStreamEvent::Done);
    Ok(())
}

fn gemini_chat_body(config: &AiChatStreamConfig, messages: &[AiChatMessage]) -> Value {
    let (system_instruction, contents) = gemini_chat_contents(messages);
    let mut body = serde_json::json!({ "contents": contents });
    if let Some(system) = system_instruction
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
    body
}

pub(crate) fn gemini_chat_contents(messages: &[AiChatMessage]) -> (Option<String>, Vec<Value>) {
    let mut system_parts = Vec::new();
    let mut contents = Vec::<(String, Vec<String>)>::new();
    for message in messages {
        match message.role {
            AiChatRole::System if !message.content.is_empty() => {
                system_parts.push(message.content.clone());
            }
            AiChatRole::System => {}
            AiChatRole::User | AiChatRole::Assistant | AiChatRole::Tool => {
                let role = if message.role == AiChatRole::Assistant {
                    "model"
                } else {
                    "user"
                };
                if let Some((last_role, parts)) = contents.last_mut()
                    && last_role == role
                {
                    parts.push(message.content.clone());
                    continue;
                }
                contents.push((role.to_string(), vec![message.content.clone()]));
            }
        }
    }
    if contents.first().is_some_and(|(role, _)| role != "user") {
        contents.insert(0, ("user".to_string(), vec!["(Continue)".to_string()]));
    }
    let contents = contents
        .into_iter()
        .map(|(role, parts)| {
            serde_json::json!({
                "role": role,
                "parts": parts.into_iter().map(|text| serde_json::json!({ "text": text })).collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();
    let system = (!system_parts.is_empty()).then(|| system_parts.join("\n\n"));
    (system, contents)
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
        }
    }
    ParsedStreamLine {
        events,
        saw_frame: true,
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
