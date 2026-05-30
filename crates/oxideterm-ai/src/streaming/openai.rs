use anyhow::{Context, Result, anyhow};
use serde_json::Value;

use crate::providers::{looks_like_html_response, openai_compatible_candidates};
use crate::{AiChatMessage, AiChatStreamConfig, AiStreamEvent};

use super::CHAT_STREAM_TIMEOUT;
use super::common::{StreamParseResult, stream_sse_response};
use super::openai_parse::{
    OpenAiToolAccumulator, parse_ollama_error, parse_openai_data_line_with_accumulator,
    parse_openai_error, parse_openai_json_events,
};
use super::openai_payload::openai_chat_body;

pub(crate) async fn stream_openai_completion(
    config: AiChatStreamConfig,
    messages: Vec<AiChatMessage>,
    events: tokio::sync::mpsc::UnboundedSender<AiStreamEvent>,
) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(CHAT_STREAM_TIMEOUT)
        .build()
        .context("failed to create AI chat client")?;
    let body = openai_chat_body(&config, &messages);
    let mut last_error = None;
    let urls = openai_compatible_candidates(&config.base_url, "/chat/completions");

    for (index, url) in urls.iter().enumerate() {
        let has_fallback = index + 1 < urls.len();
        let response = openai_stream_request(&client, url, &config, &body).await;
        let response = match response {
            Ok(response) => response,
            Err(error) if has_fallback => {
                last_error = Some(error.to_string());
                continue;
            }
            Err(error) => return Err(error),
        };

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let error_text = response.text().await.unwrap_or_default();
            let parsed = parse_openai_error(status, &error_text);
            if body.get("tool_choice").is_some() && is_tool_choice_unsupported_error(&parsed) {
                let fallback_body = body_without_tool_choice(&body);
                let retry_response =
                    openai_stream_request(&client, url, &config, &fallback_body).await;
                let retry_response = match retry_response {
                    Ok(response) => response,
                    Err(error) if has_fallback => {
                        last_error = Some(error.to_string());
                        continue;
                    }
                    Err(error) => return Err(error),
                };
                if retry_response.status().is_success() {
                    match stream_openai_response(retry_response, &events).await? {
                        StreamParseResult::Done | StreamParseResult::SawEvent => {
                            let _ = events.send(AiStreamEvent::Done);
                            return Ok(());
                        }
                        StreamParseResult::Empty { raw } => {
                            if has_fallback
                                && (raw.trim().is_empty() || looks_like_html_response(&raw))
                            {
                                continue;
                            }
                            if raw.trim().is_empty() {
                                return Err(anyhow!(
                                    "Provider returned an empty successful response ({url})."
                                ));
                            }
                            if looks_like_html_response(&raw) {
                                return Err(anyhow!(
                                    "Provider returned HTML instead of OpenAI SSE. Check the provider Base URL; OpenAI-compatible endpoints usually end with /v1. ({url})"
                                ));
                            }
                            let parsed = parse_openai_json_events(&raw, "OpenAI chat response")?;
                            if parsed.is_empty() {
                                return Err(anyhow!(
                                    "Provider returned a successful response without content ({url})."
                                ));
                            }
                            for event in parsed {
                                let _ = events.send(event);
                            }
                            let _ = events.send(AiStreamEvent::Done);
                            return Ok(());
                        }
                    }
                }
                let retry_status = retry_response.status().as_u16();
                let retry_error_text = retry_response.text().await.unwrap_or_default();
                let retry_parsed = parse_openai_error(retry_status, &retry_error_text);
                if has_fallback
                    && (retry_status == 400
                        || retry_status == 404
                        || retry_status == 405
                        || looks_like_html_response(&retry_error_text)
                        || looks_like_html_response(&retry_parsed))
                {
                    last_error = Some(retry_parsed);
                    continue;
                }
                return Err(anyhow!(retry_parsed));
            }
            if has_fallback
                && (status == 400
                    || status == 404
                    || status == 405
                    || looks_like_html_response(&error_text)
                    || looks_like_html_response(&parsed))
            {
                last_error = Some(parsed);
                continue;
            }
            return Err(anyhow!(parsed));
        }

        match stream_openai_response(response, &events).await? {
            StreamParseResult::Done | StreamParseResult::SawEvent => {
                let _ = events.send(AiStreamEvent::Done);
                return Ok(());
            }
            StreamParseResult::Empty { raw } => {
                if has_fallback && (raw.trim().is_empty() || looks_like_html_response(&raw)) {
                    continue;
                }
                if raw.trim().is_empty() {
                    return Err(anyhow!(
                        "Provider returned an empty successful response ({url})."
                    ));
                }
                if looks_like_html_response(&raw) {
                    return Err(anyhow!(
                        "Provider returned HTML instead of OpenAI SSE. Check the provider Base URL; OpenAI-compatible endpoints usually end with /v1. ({url})"
                    ));
                }
                let parsed = parse_openai_json_events(&raw, "OpenAI chat response")?;
                if parsed.is_empty() {
                    return Err(anyhow!(
                        "Provider returned a successful response without content ({url})."
                    ));
                }
                for event in parsed {
                    let _ = events.send(event);
                }
                let _ = events.send(AiStreamEvent::Done);
                return Ok(());
            }
        }
    }

    Err(anyhow!(
        last_error.unwrap_or_else(|| "No response body".to_string())
    ))
}

pub(crate) async fn stream_ollama_completion(
    mut config: AiChatStreamConfig,
    messages: Vec<AiChatMessage>,
    events: tokio::sync::mpsc::UnboundedSender<AiStreamEvent>,
) -> Result<()> {
    config.base_url = config.base_url.trim_end_matches('/').to_string();
    let url = format!("{}/v1/chat/completions", config.base_url);
    let client = reqwest::Client::builder()
        .timeout(CHAT_STREAM_TIMEOUT)
        .build()
        .context("failed to create Ollama chat client")?;
    let body = openai_chat_body(&config, &messages);
    let mut response = openai_stream_request(&client, &url, &config, &body)
        .await
        .map_err(|_| {
            anyhow!("Cannot connect to Ollama. Make sure Ollama is running (ollama serve).")
        })?;
    if !response.status().is_success() {
        let status = response.status().as_u16();
        let error_text = response.text().await.unwrap_or_default();
        let parsed = parse_ollama_error(status, &error_text);
        if body.get("tool_choice").is_some() && is_tool_choice_unsupported_error(&parsed) {
            let fallback_body = body_without_tool_choice(&body);
            response = openai_stream_request(&client, &url, &config, &fallback_body)
                .await
                .map_err(|_| {
                    anyhow!("Cannot connect to Ollama. Make sure Ollama is running (ollama serve).")
                })?;
            if !response.status().is_success() {
                let retry_status = response.status().as_u16();
                let retry_error_text = response.text().await.unwrap_or_default();
                return Err(anyhow!(parse_ollama_error(retry_status, &retry_error_text)));
            }
        } else {
            return Err(anyhow!(parsed));
        }
    }
    let _ = stream_openai_response(response, &events).await?;
    let _ = events.send(AiStreamEvent::Done);
    Ok(())
}

async fn openai_stream_request(
    client: &reqwest::Client,
    url: &str,
    config: &AiChatStreamConfig,
    body: &Value,
) -> Result<reqwest::Response> {
    let mut request = client
        .post(url)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .json(body);
    if let Some(api_key) = config.api_key.as_ref().filter(|key| !key.is_empty()) {
        request = request.bearer_auth(api_key.as_str());
    }
    request
        .send()
        .await
        .map_err(|error| anyhow!("failed to connect to AI provider: {}", error.without_url()))
}

async fn stream_openai_response(
    response: reqwest::Response,
    events: &tokio::sync::mpsc::UnboundedSender<AiStreamEvent>,
) -> Result<StreamParseResult> {
    let mut accumulator = OpenAiToolAccumulator::default();
    stream_sse_response(response, events, |line| {
        parse_openai_data_line_with_accumulator(line, &mut accumulator)
    })
    .await
}

fn body_without_tool_choice(body: &Value) -> Value {
    let mut fallback_body = body.clone();
    if let Some(object) = fallback_body.as_object_mut() {
        object.remove("tool_choice");
    }
    fallback_body
}

fn is_tool_choice_unsupported_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("tool_choice")
        || lower.contains("tool-choice")
        || (lower.contains("unsupported") && lower.contains("tool"))
        || (lower.contains("unknown") && lower.contains("tool"))
        || (lower.contains("unrecognized") && lower.contains("tool"))
        || (lower.contains("invalid") && lower.contains("tool_choice"))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{body_without_tool_choice, is_tool_choice_unsupported_error};

    #[test]
    fn tool_choice_fallback_removes_only_tool_choice() {
        let body = json!({
            "model": "model",
            "stream": true,
            "tool_choice": "required",
            "tools": [{ "type": "function" }],
        });

        let fallback = body_without_tool_choice(&body);

        assert!(fallback.get("tool_choice").is_none());
        assert_eq!(fallback.get("tools"), body.get("tools"));
        assert_eq!(
            body.get("tool_choice").and_then(|value| value.as_str()),
            Some("required")
        );
    }

    #[test]
    fn detects_tool_choice_unsupported_errors_like_tauri() {
        assert!(is_tool_choice_unsupported_error("unsupported tool choice"));
        assert!(is_tool_choice_unsupported_error(
            "Unknown parameter: tool_choice"
        ));
        assert!(is_tool_choice_unsupported_error(
            "unrecognized tool call option"
        ));
        assert!(!is_tool_choice_unsupported_error("invalid API key"));
    }
}
