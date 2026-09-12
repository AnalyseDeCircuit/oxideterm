use anyhow::{Context, Result, bail};
use eventsource_stream::Eventsource;
use futures_util::StreamExt;

use super::{
    CHAT_STREAM_TIMEOUT, responses_parse::ResponsesStream, responses_payload::responses_body,
};
use crate::{AiChatMessage, AiChatStreamConfig, AiStreamEvent};

pub(super) async fn stream_responses(
    config: AiChatStreamConfig,
    messages: Vec<AiChatMessage>,
    events: tokio::sync::mpsc::UnboundedSender<AiStreamEvent>,
) -> Result<()> {
    let client = oxideterm_network_proxy::application_http_client()
        .context("failed to acquire AI client")?;
    // The serialized request is scoped to this cancellable future and cleared on drop.
    let body = bytes::Bytes::from_owner(zeroize::Zeroizing::new(serde_json::to_vec(
        &responses_body(&config, &messages),
    )?));
    let urls = crate::providers::openai_compatible_candidates(&config.base_url, "/responses");
    let scope = config.response_state_key();
    for (index, url) in urls.iter().enumerate() {
        let mut request = client
            .post(url)
            // xAI documents hour-long request timeouts for its reasoning models; cancellation remains task-owned.
            .timeout(if config.provider_type == "xai" {
                std::time::Duration::from_secs(3600)
            } else {
                CHAT_STREAM_TIMEOUT
            })
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header(reqwest::header::ACCEPT, "text/event-stream")
            .body(body.clone());
        if let Some(key) = config.api_key.as_ref().filter(|key| !key.is_empty()) {
            request = request.bearer_auth(key.as_str());
        }
        let response = request.send().await.map_err(|error| anyhow::Error::new(error.without_url()))?;
        if !response.status().is_success() {
            super::retry::check_transient_response(&response)?;
            let status = response.status();
            if matches!(status.as_u16(), 404 | 405) && index + 1 < urls.len() {
                continue;
            }
            bail!("Responses request failed (HTTP {})", status.as_u16());
        }
        let mut parser = ResponsesStream::default();
        if response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.contains("application/json"))
        {
            let response: serde_json::Value = response
                .json()
                .await
                .context("Responses provider returned invalid JSON")?;
            let kind = match response["status"].as_str() {
                Some("completed") => "response.completed",
                Some("incomplete") => "response.incomplete",
                _ => "response.failed",
            };
            for event in
                parser.event(serde_json::json!({"type":kind,"response":response}), &scope)?
            {
                if events.send(event).is_err() {
                    return Ok(());
                }
            }
            return Ok(());
        }
        let source = response.bytes_stream().eventsource();
        futures_util::pin_mut!(source);
        while let Some(frame) = source.next().await {
            let frame = frame.map_err(|_| {
                anyhow::anyhow!("Responses stream transport or SSE decoding failed")
            })?;
            let value = serde_json::from_str(&frame.data)
                .map_err(|_| anyhow::anyhow!("Responses provider returned invalid event JSON"))?;
            for event in parser.event(value, &scope)? {
                let done = matches!(event, AiStreamEvent::Done | AiStreamEvent::Error(_));
                if events.send(event).is_err() || done {
                    return Ok(());
                }
            }
        }
        bail!("responses_disconnected");
    }
    bail!("No Responses endpoint configured")
}
