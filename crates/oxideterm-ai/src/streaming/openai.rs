use anyhow::{Context, Result, anyhow};
use serde_json::Value;
use zeroize::Zeroizing;

use crate::providers::{looks_like_html_response, openai_compatible_candidates};
use crate::{AiChatMessage, AiChatStreamConfig, AiStreamEvent};

use super::CHAT_STREAM_TIMEOUT;
use super::common::{
    ErrorEnvelope, ProviderError, StreamParseResult, read_provider_error_body, stream_sse_response,
};
use super::openai_parse::{
    OpenAiToolAccumulator, chat_provider_error, parse_ollama_error,
    parse_openai_data_line_with_accumulator, parse_openai_json_events,
};
use super::openai_payload::openai_chat_body;

pub(crate) async fn stream_openai_completion(
    config: AiChatStreamConfig,
    messages: Vec<AiChatMessage>,
    events: tokio::sync::mpsc::UnboundedSender<AiStreamEvent>,
) -> Result<()> {
    // Reuse the application pool across tool rounds; provider keys remain request-scoped below.
    let client = oxideterm_network_proxy::application_http_client()
        .context("failed to acquire application AI chat client")?;
    let body = openai_chat_body(&config, &messages);
    let urls = openai_compatible_candidates(&config.base_url, "/chat/completions");

    for (index, url) in urls.iter().enumerate() {
        let has_fallback = index + 1 < urls.len();
        let mut response = match openai_stream_request(&client, url, &config, &body).await {
            Ok(response) => response,
            Err(_) if has_fallback => continue,
            Err(error) => return Err(error),
        };

        if !response.status().is_success() {
            super::retry::check_transient_response(&response)?;
            let mut rejection =
                chat_rejection(&mut response, body.get("tool_choice").is_some()).await;
            if rejection.unsupported_tool_choice {
                let fallback_body = body_without_tool_choice(&body);
                response =
                    match openai_stream_request(&client, url, &config, &fallback_body).await {
                        Ok(response) => response,
                        Err(_) if has_fallback => continue,
                        Err(error) => return Err(error),
                    };
                super::retry::check_transient_response(&response)?;
                if !response.status().is_success() {
                    rejection = chat_rejection(&mut response, false).await;
                }
            }
            if !response.status().is_success() {
                if has_fallback && rejection.try_alternative_endpoint {
                    continue;
                }
                return Err(anyhow!(rejection.reason));
            }
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
                    return Err(anyhow!("chat_invalid_stream"));
                }
                if looks_like_html_response(&raw) {
                    return Err(anyhow!("chat_endpoint"));
                }
                let parsed = parse_openai_json_events(&raw, "OpenAI chat response")
                    .map_err(|_| anyhow!("chat_invalid_stream"))?;
                if parsed.is_empty() {
                    return Err(anyhow!("chat_invalid_stream"));
                }
                for event in parsed {
                    let _ = events.send(event);
                }
                let _ = events.send(AiStreamEvent::Done);
                return Ok(());
            }
        }
    }

    Err(anyhow!("stream_failed"))
}

struct ChatRejection {
    reason: String,
    try_alternative_endpoint: bool,
    unsupported_tool_choice: bool,
}

async fn chat_rejection(response: &mut reqwest::Response, has_tool_choice: bool) -> ChatRejection {
    let status = response.status().as_u16();
    let body = read_provider_error_body(response).await;
    let raw = std::str::from_utf8(&body).unwrap_or_default();
    let details = serde_json::from_slice::<ErrorEnvelope<'_>>(&body)
        .ok()
        .and_then(|envelope| envelope.error)
        .or_else(|| serde_json::from_slice::<ProviderError<'_>>(&body).ok());
    let message = details.as_ref().and_then(|details| details.message.as_deref());
    let code = details.as_ref().and_then(|details| details.code.as_deref());
    let classified = chat_provider_error(code, message);
    let is_html = looks_like_html_response(raw) || message.is_some_and(looks_like_html_response);
    let category = classified.unwrap_or_else(|| {
        if is_html && status == 400 {
            "chat_endpoint"
        } else {
            chat_http_category(status)
        }
    });
    ChatRejection {
        reason: format!("{category}:{status}"),
        // A known semantic rejection will not be fixed by trying a different path.
        try_alternative_endpoint: classified.is_none()
            && (matches!(status, 400 | 404 | 405) || is_html),
        unsupported_tool_choice: has_tool_choice
            && is_tool_choice_unsupported_error(message.unwrap_or(raw)),
    }
}

fn chat_http_category(status: u16) -> &'static str {
    match status {
        401 | 403 => "chat_auth",
        402 => "chat_balance",
        404 | 405 => "chat_endpoint",
        408 | 504 => "chat_timeout",
        429 => "chat_rate_limit",
        500..=599 => "chat_unavailable",
        _ => "chat_rejected",
    }
}

pub(super) fn safe_chat_error(error: &anyhow::Error) -> String {
    if let Some(failure) = error.downcast_ref::<super::retry::TransientFailure>() {
        return format!("{}:{}", chat_http_category(failure.status), failure.status);
    }
    if let Some(error) = error.downcast_ref::<reqwest::Error>() {
        return if error.is_timeout() {
            "chat_timeout"
        } else if error.is_body() {
            "ai_stream_interrupted"
        } else {
            "chat_network"
        }
        .into();
    }
    let code = error.to_string();
    if super::stream_error_label(&code).is_some() {
        code
    } else {
        "stream_failed".into()
    }
}

pub(crate) async fn stream_ollama_completion(
    mut config: AiChatStreamConfig,
    messages: Vec<AiChatMessage>,
    events: tokio::sync::mpsc::UnboundedSender<AiStreamEvent>,
) -> Result<()> {
    config.base_url = config.base_url.trim_end_matches('/').to_string();
    let url = format!("{}/v1/chat/completions", config.base_url);
    // Reuse the same application pool for every model round in this conversation.
    let client = oxideterm_network_proxy::application_http_client()
        .context("failed to acquire application Ollama chat client")?;
    let body = openai_chat_body(&config, &messages);
    let mut response = openai_stream_request(&client, &url, &config, &body)
        .await
        .map_err(|_| {
            anyhow!("Cannot connect to Ollama. Make sure Ollama is running (ollama serve).")
        })?;
    if !response.status().is_success() {
        super::retry::check_transient_response(&response)?;
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
                super::retry::check_transient_response(&response)?;
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
        // xAI documents hour-long request timeouts for its reasoning models; cancellation remains task-owned.
        .timeout(if config.provider_type == "xai" {
            std::time::Duration::from_secs(3600)
        } else {
            CHAT_STREAM_TIMEOUT
        })
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .json(body);
    if let Some(api_key) = config.api_key.as_ref().filter(|key| !key.is_empty()) {
        request = request.bearer_auth(api_key.as_str());
    }
    request
        .send()
        .await
        .map_err(|error| anyhow::Error::new(error.without_url()))
}

async fn stream_openai_response(
    response: reqwest::Response,
    events: &tokio::sync::mpsc::UnboundedSender<AiStreamEvent>,
) -> Result<StreamParseResult> {
    let mut accumulator = OpenAiToolAccumulator::default();
    let result = stream_sse_response(response, events, |line| {
        parse_openai_data_line_with_accumulator(line, &mut accumulator)
    })
    .await?;
    if matches!(result, StreamParseResult::SawEvent) && !accumulator.finished {
        return Err(anyhow!("ai_stream_interrupted"));
    }
    Ok(result)
}

fn body_without_tool_choice(body: &Value) -> Value {
    let mut fallback_body = body.clone();
    if let Some(object) = fallback_body.as_object_mut() {
        object.remove("tool_choice");
    }
    fallback_body
}

fn is_tool_choice_unsupported_error(message: &str) -> bool {
    let lower = Zeroizing::new(message.to_ascii_lowercase());
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

    use super::body_without_tool_choice;

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
}
