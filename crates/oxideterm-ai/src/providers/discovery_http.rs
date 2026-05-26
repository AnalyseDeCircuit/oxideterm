use anyhow::{Result, anyhow};
use serde_json::Value;
use zeroize::Zeroizing;

use crate::AiProviderView;

pub(crate) const ANTHROPIC_VERSION: &str = "2023-06-01";

pub(crate) async fn fetch_provider_models_payload(
    client: &reqwest::Client,
    provider: &AiProviderView,
    api_key: Option<&Zeroizing<String>>,
) -> Result<Value> {
    let provider_type = provider.provider_type.as_str();
    match provider_type {
        "anthropic" => {
            let api_key = api_key_required_ref(provider_type, api_key)?;
            // The key is owned by Zeroizing until the request is built. Reqwest
            // must copy it into an HTTP header to match Tauri's provider API
            // request path, and native never persists this value in settings.
            let body = send_text(
                client
                    .get(provider_models_url(provider_type, &provider.base_url)?)
                    .header("x-api-key", api_key.as_str())
                    .header("anthropic-version", ANTHROPIC_VERSION),
            )
            .await?;
            parse_provider_json(&body, "Anthropic model list")
        }
        "gemini" => {
            let api_key = api_key_required_ref(provider_type, api_key)?;
            // Keep the API key out of our own URL strings; reqwest owns the
            // query parameter only for the outgoing request.
            let body = send_text(
                client
                    .get(gemini_models_url(&provider.base_url)?)
                    .query(&[("key", api_key.as_str())]),
            )
            .await?;
            parse_provider_json(&body, "Gemini model list")
        }
        "ollama" => {
            let body =
                send_text(client.get(provider_models_url(provider_type, &provider.base_url)?))
                    .await
                    .map_err(|_| {
                        anyhow!(
                            "Cannot connect to Ollama. Make sure Ollama is running (ollama serve)."
                        )
                    })?;
            parse_provider_json(&body, "Ollama model list")
        }
        _ => {
            fetch_openai_compatible_json(
                client,
                &provider.base_url,
                "/models",
                api_key,
                if provider_type == "openai" {
                    "OpenAI model list"
                } else {
                    "OpenAI-compatible model list"
                },
            )
            .await
        }
    }
}

async fn send_text(request: reqwest::RequestBuilder) -> Result<String> {
    let response = request
        .send()
        .await
        .map_err(|error| anyhow!("model refresh request failed: {}", error.without_url()))?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(anyhow!("model refresh failed with HTTP {status}"));
    }
    Ok(body)
}

pub(crate) fn parse_provider_json(body: &str, context: &str) -> Result<Value> {
    serde_json::from_str(body).map_err(|error| {
        if looks_like_html_response(body) {
            anyhow!(
                "{context} returned HTML instead of JSON. Check the provider Base URL; OpenAI-compatible endpoints usually end with /v1."
            )
        } else {
            anyhow!("{context} returned invalid JSON: {error}")
        }
    })
}

async fn fetch_openai_compatible_json(
    client: &reqwest::Client,
    base_url: &str,
    path: &str,
    api_key: Option<&Zeroizing<String>>,
    context: &str,
) -> Result<Value> {
    let candidates = openai_compatible_candidates(base_url, path);
    let mut errors = Vec::new();
    for (index, url) in candidates.iter().enumerate() {
        let mut request = client.get(url);
        if let Some(api_key) = api_key.filter(|key| !key.trim().is_empty()) {
            request = request.bearer_auth(api_key.as_str());
        }
        let has_fallback = index + 1 < candidates.len();
        let response = request
            .send()
            .await
            .map_err(|error| anyhow!("{context} request failed: {}", error.without_url()))?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            let message = format!("{url} returned HTTP {status}");
            errors.push(message.clone());
            if has_fallback
                && (status.as_u16() == 400
                    || status.as_u16() == 404
                    || status.as_u16() == 405
                    || looks_like_html_response(&body))
            {
                continue;
            }
            return Err(anyhow!("{context} failed: {message}"));
        }
        match parse_provider_json(&body, context) {
            Ok(json) => return Ok(json),
            Err(error) => {
                errors.push(format!("{url}: {error}"));
                if has_fallback && looks_like_html_response(&body) {
                    continue;
                }
                return Err(error);
            }
        }
    }
    Err(anyhow!("{context} failed. {}", errors.join("; ")))
}

pub(crate) fn openai_compatible_candidates(base_url: &str, path: &str) -> Vec<String> {
    let clean_base_url = base_url.trim().trim_end_matches('/');
    let mut candidates = vec![format!("{clean_base_url}{path}")];
    if let Ok(parsed) = reqwest::Url::parse(clean_base_url) {
        let pathname = parsed.path().trim_end_matches('/');
        if !path_has_version_segment(pathname) {
            candidates.push(format!("{clean_base_url}/v1{path}"));
        }
    }
    candidates.dedup();
    candidates
}

fn path_has_version_segment(pathname: &str) -> bool {
    pathname.split('/').any(|segment| {
        segment.len() >= 2
            && segment.starts_with('v')
            && segment[1..].chars().all(|ch| ch.is_ascii_digit())
    })
}

pub(crate) fn looks_like_html_response(body: &str) -> bool {
    body.trim_start().starts_with('<')
}

pub(crate) fn api_key_required_ref<'a>(
    provider_type: &str,
    api_key: Option<&'a Zeroizing<String>>,
) -> Result<&'a Zeroizing<String>> {
    api_key
        .filter(|key| !key.trim().is_empty())
        .ok_or_else(|| anyhow!("missing API key for {provider_type}"))
}

fn provider_models_url(provider_type: &str, base_url: &str) -> Result<String> {
    let base_url = base_url.trim().trim_end_matches('/');
    if base_url.is_empty() {
        return Err(anyhow!("provider base URL is empty"));
    }
    let suffix = match provider_type {
        "anthropic" => "v1/models",
        "gemini" => "models",
        "ollama" => "api/tags",
        _ => "models",
    };
    Ok(format!("{base_url}/{suffix}"))
}

fn gemini_models_url(base_url: &str) -> Result<String> {
    let base_url = base_url.trim().trim_end_matches('/');
    if base_url.is_empty() {
        return Err(anyhow!("provider base URL is empty"));
    }
    Ok(format!("{base_url}/v1beta/models"))
}

pub(crate) fn url_encode_component(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                vec![byte as char]
            }
            _ => format!("%{byte:02X}").chars().collect(),
        })
        .collect()
}
