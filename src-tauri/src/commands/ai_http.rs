// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! AI HTTP Proxy Commands
//!
//! Proxies AI provider API calls through the Rust backend to bypass
//! browser CORS restrictions. Essential for local providers like
//! LM Studio, Ollama, and any OpenAI-compatible server.
//!
//! Security:
//! - Only HTTP/HTTPS URLs allowed (prevents file:// SSRF)
//! - Non-streaming response body capped at 10 MB
//! - Streaming channel auto-closes on frontend disconnect

use base64::{Engine as _, engine::general_purpose::STANDARD};
use dashmap::DashMap;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::State;
use tauri::ipc::Channel;
use tokio_util::sync::CancellationToken;

/// Maximum response body size for non-streaming requests (10 MB)
const MAX_RESPONSE_SIZE: usize = 10 * 1024 * 1024;

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

/// Response from a non-streaming AI fetch
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiFetchResponse {
    pub status: u16,
    pub body: String,
}

/// Binary-safe HTTP response for plugin API calls.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginHttpResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body_base64: String,
}

/// Chunk events for streaming AI fetch
#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AiStreamEvent {
    /// HTTP status code (sent first, before any data)
    Status { code: u16 },
    /// A chunk of response body data
    Data { data: String },
    /// An error occurred during streaming
    Error { message: String },
}

/// Registry of in-flight streaming requests that can be cancelled.
pub type AiStreamCancelRegistry = Arc<DashMap<String, CancellationToken>>;

// ═══════════════════════════════════════════════════════════════════════════
// Validation
// ═══════════════════════════════════════════════════════════════════════════

/// Validate that the URL uses HTTP or HTTPS scheme only.
fn validate_url(url: &str) -> Result<(), String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("Only HTTP and HTTPS URLs are supported".into());
    }
    Ok(())
}

/// Build a reqwest request from common parameters.
fn build_request(
    client: &reqwest::Client,
    url: &str,
    method: &str,
    headers: &HashMap<String, String>,
    body: Option<String>,
) -> Result<reqwest::RequestBuilder, String> {
    let mut builder = match method.to_uppercase().as_str() {
        "GET" => client.get(url),
        "POST" => client.post(url),
        "PUT" => client.put(url),
        "DELETE" => client.delete(url),
        "PATCH" => client.patch(url),
        other => return Err(format!("Unsupported HTTP method: {}", other)),
    };

    for (key, value) in headers {
        builder = builder.header(key.as_str(), value.as_str());
    }

    if let Some(body_str) = body {
        builder = builder.body(body_str);
    }

    Ok(builder)
}

fn build_request_binary(
    client: &reqwest::Client,
    url: &str,
    method: &str,
    headers: &HashMap<String, String>,
    body: Option<Vec<u8>>,
) -> Result<reqwest::RequestBuilder, String> {
    let mut builder = match method.to_uppercase().as_str() {
        "GET" => client.get(url),
        "POST" => client.post(url),
        "PUT" => client.put(url),
        "DELETE" => client.delete(url),
        "PATCH" => client.patch(url),
        "MKCOL" => client.request(
            reqwest::Method::from_bytes(b"MKCOL").map_err(|e| e.to_string())?,
            url,
        ),
        "PROPFIND" => client.request(
            reqwest::Method::from_bytes(b"PROPFIND").map_err(|e| e.to_string())?,
            url,
        ),
        "HEAD" => client.head(url),
        other => return Err(format!("Unsupported HTTP method: {}", other)),
    };

    for (key, value) in headers {
        builder = builder.header(key.as_str(), value.as_str());
    }

    if let Some(body_bytes) = body {
        builder = builder.body(body_bytes);
    }

    Ok(builder)
}

// ═══════════════════════════════════════════════════════════════════════════
// Tauri Commands
// ═══════════════════════════════════════════════════════════════════════════

/// Non-streaming HTTP fetch for AI API calls (bypasses browser CORS).
///
/// Used for model listing, model details, and other non-streaming API calls.
#[tauri::command]
pub async fn ai_fetch(
    url: String,
    method: String,
    headers: HashMap<String, String>,
    body: Option<String>,
    timeout_ms: Option<u64>,
) -> Result<AiFetchResponse, String> {
    validate_url(&url)?;

    tracing::debug!(
        "[ai_fetch] {} {} (timeout: {:?}ms)",
        method,
        url,
        timeout_ms
    );

    let client = reqwest::Client::new();
    let mut builder = build_request(&client, &url, &method, &headers, body)?;

    if let Some(ms) = timeout_ms {
        if ms > 0 {
            builder = builder.timeout(std::time::Duration::from_millis(ms));
        }
    }

    let response = builder
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    let status = response.status().as_u16();
    tracing::debug!("[ai_fetch] {} {} → {}", method, url, status);

    // Read body with size limit
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    if bytes.len() > MAX_RESPONSE_SIZE {
        return Err(format!(
            "Response too large: {} bytes (max {} bytes)",
            bytes.len(),
            MAX_RESPONSE_SIZE
        ));
    }

    let body = String::from_utf8_lossy(&bytes).to_string();

    Ok(AiFetchResponse { status, body })
}

/// Binary-safe HTTP proxy for plugins (bypasses browser CORS).
#[tauri::command]
pub async fn plugin_http_request(
    url: String,
    method: String,
    headers: HashMap<String, String>,
    body_base64: Option<String>,
    timeout_ms: Option<u64>,
) -> Result<PluginHttpResponse, String> {
    validate_url(&url)?;

    let body = match body_base64 {
        Some(encoded) if !encoded.is_empty() => {
            let decoded = STANDARD
                .decode(encoded)
                .map_err(|e| format!("Invalid base64 request body: {}", e))?;
            if decoded.len() > MAX_RESPONSE_SIZE {
                return Err(format!(
                    "Request body too large: {} bytes (max {} bytes)",
                    decoded.len(),
                    MAX_RESPONSE_SIZE
                ));
            }
            Some(decoded)
        }
        _ => None,
    };

    tracing::debug!(
        "[plugin_http_request] {} {} (timeout: {:?}ms)",
        method,
        url,
        timeout_ms
    );

    let client = reqwest::Client::new();
    let mut builder = build_request_binary(&client, &url, &method, &headers, body)?;

    if let Some(ms) = timeout_ms {
        if ms > 0 {
            builder = builder.timeout(std::time::Duration::from_millis(ms));
        }
    }

    let response = builder
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    let status = response.status().as_u16();
    let response_headers = response
        .headers()
        .iter()
        .map(|(key, value)| {
            (
                key.to_string(),
                value.to_str().unwrap_or_default().to_string(),
            )
        })
        .collect::<HashMap<_, _>>();

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    if bytes.len() > MAX_RESPONSE_SIZE {
        return Err(format!(
            "Response too large: {} bytes (max {} bytes)",
            bytes.len(),
            MAX_RESPONSE_SIZE
        ));
    }

    Ok(PluginHttpResponse {
        status,
        headers: response_headers,
        body_base64: STANDARD.encode(bytes),
    })
}

/// Streaming HTTP fetch for AI API calls (bypasses browser CORS).
///
/// Sends response chunks through a Tauri Channel for SSE streaming.
/// The first event is always a `Status` with the HTTP status code,
/// followed by `Data` chunks, and the command returns when the stream ends.
#[tauri::command]
pub async fn ai_fetch_stream(
    request_id: String,
    url: String,
    method: String,
    headers: HashMap<String, String>,
    body: Option<String>,
    on_chunk: Channel<AiStreamEvent>,
    cancel_registry: State<'_, AiStreamCancelRegistry>,
) -> Result<(), String> {
    validate_url(&url)?;

    // Register a cancellation token for this request
    let token = CancellationToken::new();
    cancel_registry.insert(request_id.clone(), token.clone());

    let result = ai_fetch_stream_inner(&url, &method, &headers, body, &on_chunk, &token).await;

    // Always clean up the token entry
    cancel_registry.remove(&request_id);

    result
}

/// Inner streaming logic, separated so the cancel registry cleanup is always reached.
async fn ai_fetch_stream_inner(
    url: &str,
    method: &str,
    headers: &HashMap<String, String>,
    body: Option<String>,
    on_chunk: &Channel<AiStreamEvent>,
    token: &CancellationToken,
) -> Result<(), String> {
    let client = reqwest::Client::new();
    let builder = build_request(&client, url, method, headers, body)?;

    let response = tokio::select! {
        biased;
        _ = token.cancelled() => return Ok(()),
        result = builder.send() => {
            result.map_err(|e| format!("HTTP streaming request failed: {}", e))?
        }
    };

    let status = response.status().as_u16();

    // Send status first so frontend can check before reading data
    if on_chunk
        .send(AiStreamEvent::Status { code: status })
        .is_err()
    {
        return Ok(()); // Channel closed
    }

    if !response.status().is_success() {
        // For error responses, read the full body and send as a single Data chunk
        let error_body = tokio::select! {
            biased;
            _ = token.cancelled() => return Ok(()),
            text = response.text() => text.unwrap_or_default(),
        };
        let data = if error_body.len() > 2000 {
            format!(
                "{}... [truncated]",
                error_body.chars().take(2000).collect::<String>()
            )
        } else {
            error_body
        };
        let _ = on_chunk.send(AiStreamEvent::Data { data });
        return Ok(());
    }

    // Stream the success response body chunk by chunk
    use futures_util::StreamExt;
    let mut stream = response.bytes_stream();

    loop {
        tokio::select! {
            biased;
            _ = token.cancelled() => break,
            chunk = stream.next() => {
                match chunk {
                    Some(Ok(bytes)) => {
                        let text = String::from_utf8_lossy(&bytes).to_string();
                        if on_chunk.send(AiStreamEvent::Data { data: text }).is_err() {
                            // Channel closed (frontend disconnected) — stop streaming
                            break;
                        }
                    }
                    Some(Err(e)) => {
                        let _ = on_chunk.send(AiStreamEvent::Error {
                            message: format!("Stream read error: {}", e),
                        });
                        break;
                    }
                    None => break, // Stream ended
                }
            }
        }
    }

    Ok(())
}

/// Cancel an in-flight AI streaming request.
#[tauri::command]
pub async fn ai_fetch_stream_cancel(
    request_id: String,
    cancel_registry: State<'_, AiStreamCancelRegistry>,
) -> Result<(), String> {
    if let Some((_, token)) = cancel_registry.remove(&request_id) {
        token.cancel();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::build_request_binary;
    use std::collections::HashMap;

    #[test]
    fn build_request_binary_accepts_webdav_methods() {
        let client = reqwest::Client::new();
        let headers = HashMap::new();

        let mkcol =
            build_request_binary(&client, "http://127.0.0.1/example", "MKCOL", &headers, None)
                .expect("MKCOL should be supported")
                .build()
                .expect("MKCOL request should build");
        assert_eq!(mkcol.method().as_str(), "MKCOL");

        let propfind = build_request_binary(
            &client,
            "http://127.0.0.1/example",
            "PROPFIND",
            &headers,
            None,
        )
        .expect("PROPFIND should be supported")
        .build()
        .expect("PROPFIND request should build");
        assert_eq!(propfind.method().as_str(), "PROPFIND");
    }
}
