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
    let response = openai_stream_request(&client, &url, &config, &body)
        .await
        .map_err(|_| {
            anyhow!("Cannot connect to Ollama. Make sure Ollama is running (ollama serve).")
        })?;
    if !response.status().is_success() {
        let status = response.status().as_u16();
        let error_text = response.text().await.unwrap_or_default();
        return Err(anyhow!(parse_ollama_error(status, &error_text)));
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
        .with_context(|| format!("failed to connect to AI provider at {url}"))
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
