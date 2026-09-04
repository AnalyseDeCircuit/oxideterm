use std::{fmt, time::Duration};

use crate::{AiProviderView, SharedAiProviderKey, provider_view, sanitize_for_ai};
use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use zeroize::Zeroize;

const EMBEDDING_TIMEOUT: Duration = Duration::from_secs(3);
const OPENAI_DEFAULT_EMBEDDING_MODEL: &str = "text-embedding-3-small";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiEmbeddingMode {
    Configured,
    Auto,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiEmbeddingProviderReason {
    Ready,
    NoProvider,
    UnsupportedProvider,
    MissingModel,
    MissingApiKey,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedAiEmbeddingProvider {
    pub provider: Option<AiProviderView>,
    pub model: String,
    pub mode: AiEmbeddingMode,
    pub reason: AiEmbeddingProviderReason,
}

#[derive(Clone)]
pub enum AiChatEmbeddingApiKeyDecision<'a> {
    UseKey(&'a SharedAiProviderKey),
    NoKey,
    LoadProviderKey(String),
    Skip,
}

impl fmt::Debug for AiChatEmbeddingApiKeyDecision<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Embedding key decisions are useful in tests/logs, but the loaded key
        // branch must never expose the provider secret.
        match self {
            Self::UseKey(_) => formatter
                .debug_tuple("UseKey")
                .field(&"<redacted>")
                .finish(),
            Self::NoKey => formatter.write_str("NoKey"),
            Self::LoadProviderKey(provider_id) => formatter
                .debug_tuple("LoadProviderKey")
                .field(provider_id)
                .finish(),
            Self::Skip => formatter.write_str("Skip"),
        }
    }
}

pub fn ai_provider_supports_embeddings(provider: &AiProviderView) -> bool {
    provider.enabled
        && matches!(
            provider.provider_type.as_str(),
            "openai" | "openai_compatible" | "ollama"
        )
}

pub fn ai_embedding_requires_api_key(provider: &AiProviderView) -> bool {
    !matches!(
        provider.provider_type.as_str(),
        "ollama" | "openai_compatible"
    )
}

pub fn resolve_ai_embedding_provider(
    providers: &[serde_json::Value],
    active_provider_id: Option<&str>,
    embedding_config: Option<&serde_json::Value>,
    has_api_key: Option<bool>,
) -> ResolvedAiEmbeddingProvider {
    let provider_views = providers
        .iter()
        .filter_map(provider_view)
        .collect::<Vec<_>>();
    let configured_provider_id = embedding_config
        .and_then(|config| config.get("providerId"))
        .and_then(serde_json::Value::as_str)
        .filter(|id| !id.trim().is_empty());
    let configured_model = embedding_config
        .and_then(|config| config.get("model"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();

    if let Some(provider_id) = configured_provider_id {
        let configured = provider_views
            .iter()
            .find(|provider| provider.id == provider_id)
            .cloned();
        let Some(provider) = configured else {
            return ResolvedAiEmbeddingProvider {
                provider: None,
                model: configured_model.to_string(),
                mode: AiEmbeddingMode::Configured,
                reason: AiEmbeddingProviderReason::UnsupportedProvider,
            };
        };
        if !ai_provider_supports_embeddings(&provider) {
            return ResolvedAiEmbeddingProvider {
                provider: Some(provider),
                model: configured_model.to_string(),
                mode: AiEmbeddingMode::Configured,
                reason: AiEmbeddingProviderReason::UnsupportedProvider,
            };
        }
        let model = ai_embedding_model(embedding_config, &provider);
        let reason = ai_embedding_reason(embedding_config, &provider, has_api_key);
        return ResolvedAiEmbeddingProvider {
            provider: Some(provider),
            model,
            mode: AiEmbeddingMode::Configured,
            reason,
        };
    }

    let active_provider = active_provider_id.and_then(|active_id| {
        provider_views
            .iter()
            .find(|provider| provider.id == active_id)
    });
    let mut candidates = Vec::new();
    if let Some(provider) =
        active_provider.filter(|provider| ai_provider_supports_embeddings(provider))
    {
        candidates.push(provider.clone());
    }
    candidates.extend(
        provider_views
            .iter()
            .filter(|provider| Some(provider.id.as_str()) != active_provider_id)
            .filter(|provider| ai_provider_supports_embeddings(provider))
            .cloned(),
    );

    let auto_provider = candidates
        .iter()
        .find(|provider| !ai_embedding_model(embedding_config, provider).is_empty())
        .cloned()
        .or_else(|| candidates.first().cloned());
    let Some(provider) = auto_provider else {
        return ResolvedAiEmbeddingProvider {
            provider: None,
            model: configured_model.to_string(),
            mode: AiEmbeddingMode::Auto,
            reason: AiEmbeddingProviderReason::NoProvider,
        };
    };
    let model = ai_embedding_model(embedding_config, &provider);
    let reason = ai_embedding_reason(embedding_config, &provider, has_api_key);
    ResolvedAiEmbeddingProvider {
        provider: Some(provider),
        model,
        mode: AiEmbeddingMode::Auto,
        reason,
    }
}

pub fn resolve_chat_embedding_api_key<'a>(
    embedding_provider_id: &str,
    active_provider_id: Option<&str>,
    // Keep the returned active key tied to the caller-owned stream config.
    active_provider_api_key: Option<&'a SharedAiProviderKey>,
    embedding_requires_api_key: bool,
    embedding_mode: AiEmbeddingMode,
) -> AiChatEmbeddingApiKeyDecision<'a> {
    if !embedding_requires_api_key {
        return AiChatEmbeddingApiKeyDecision::NoKey;
    }

    if Some(embedding_provider_id) == active_provider_id {
        return active_provider_api_key
            .filter(|key| !key.as_str().trim().is_empty())
            .map(AiChatEmbeddingApiKeyDecision::UseKey)
            .unwrap_or(AiChatEmbeddingApiKeyDecision::Skip);
    }

    if embedding_mode == AiEmbeddingMode::Configured {
        return AiChatEmbeddingApiKeyDecision::LoadProviderKey(embedding_provider_id.to_string());
    }

    AiChatEmbeddingApiKeyDecision::Skip
}

pub async fn embed_texts(
    provider: &AiProviderView,
    api_key: Option<&SharedAiProviderKey>,
    model: &str,
    texts: Vec<String>,
) -> Result<Vec<Vec<f32>>> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }
    // This is the single outbound boundary for document chunks and search
    // queries. Consume and clear each raw allocation before provider dispatch.
    let texts = sanitize_embedding_texts(texts);
    dispatch_sanitized_embeddings(provider, api_key, model, texts).await
}

pub async fn embed_query_text(
    provider: &AiProviderView,
    api_key: Option<&SharedAiProviderKey>,
    model: &str,
    query: &str,
) -> Result<Vec<Vec<f32>>> {
    // A query remains borrowed by the local keyword search, so sanitize it
    // directly at this boundary without first cloning the raw query.
    let texts = sanitize_embedding_query(query);
    dispatch_sanitized_embeddings(provider, api_key, model, texts).await
}

async fn dispatch_sanitized_embeddings(
    provider: &AiProviderView,
    api_key: Option<&SharedAiProviderKey>,
    model: &str,
    texts: Vec<String>,
) -> Result<Vec<Vec<f32>>> {
    match provider.provider_type.as_str() {
        "openai" | "openai_compatible" => {
            embed_openai_compatible(&provider.base_url, api_key, model, texts).await
        }
        "ollama" => embed_ollama(&provider.base_url, api_key, model, texts).await,
        provider_type => Err(anyhow!(
            "unsupported embedding provider type: {provider_type}"
        )),
    }
}

fn sanitize_embedding_texts(texts: Vec<String>) -> Vec<String> {
    texts
        .into_iter()
        .map(|mut raw_text| {
            let sanitized_text = sanitize_for_ai(&raw_text);
            raw_text.zeroize();
            sanitized_text
        })
        .collect()
}

fn sanitize_embedding_query(query: &str) -> Vec<String> {
    vec![sanitize_for_ai(query)]
}

fn ai_embedding_model(
    embedding_config: Option<&serde_json::Value>,
    provider: &AiProviderView,
) -> String {
    let configured = embedding_config
        .and_then(|config| config.get("model"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|model| !model.is_empty());
    if let Some(model) = configured {
        return model.to_string();
    }
    if provider.provider_type == "openai" {
        return OPENAI_DEFAULT_EMBEDDING_MODEL.to_string();
    }
    String::new()
}

fn ai_embedding_reason(
    embedding_config: Option<&serde_json::Value>,
    provider: &AiProviderView,
    has_api_key: Option<bool>,
) -> AiEmbeddingProviderReason {
    if ai_embedding_model(embedding_config, provider).is_empty() {
        return AiEmbeddingProviderReason::MissingModel;
    }
    if ai_embedding_requires_api_key(provider) && has_api_key == Some(false) {
        return AiEmbeddingProviderReason::MissingApiKey;
    }
    AiEmbeddingProviderReason::Ready
}

async fn embed_openai_compatible(
    base_url: &str,
    api_key: Option<&SharedAiProviderKey>,
    model: &str,
    texts: Vec<String>,
) -> Result<Vec<Vec<f32>>> {
    let client = embedding_client()?;
    let url = format!("{}/embeddings", base_url.trim_end_matches('/'));
    let mut request = client
        .post(&url)
        .timeout(EMBEDDING_TIMEOUT)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .json(&serde_json::json!({ "model": model, "input": texts }));
    if let Some(api_key) = api_key.as_ref().filter(|key| !key.is_empty()) {
        request = request.bearer_auth(api_key.as_str());
    }
    let response = request.send().await.map_err(|error| {
        anyhow!(
            "failed to connect to embedding provider: {}",
            error.without_url()
        )
    })?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(anyhow!("Embedding request failed: {}", status.as_u16()));
    }
    let parsed: OpenAiEmbeddingResponse =
        serde_json::from_str(&body).context("Invalid embedding response")?;
    let mut data = parsed.data;
    data.sort_by_key(|item| item.index);
    Ok(data.into_iter().map(|item| item.embedding).collect())
}

async fn embed_ollama(
    base_url: &str,
    api_key: Option<&SharedAiProviderKey>,
    model: &str,
    texts: Vec<String>,
) -> Result<Vec<Vec<f32>>> {
    let client = embedding_client()?;
    let url = format!("{}/api/embed", base_url.trim_end_matches('/'));
    let mut request = client
        .post(&url)
        .timeout(EMBEDDING_TIMEOUT)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .json(&serde_json::json!({ "model": model, "input": texts }));
    if let Some(api_key) = api_key.as_ref().filter(|key| !key.is_empty()) {
        request = request.bearer_auth(api_key.as_str());
    }
    let response = request.send().await.map_err(|error| {
        anyhow!(
            "failed to connect to Ollama embedding endpoint: {}",
            error.without_url()
        )
    })?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(anyhow!(
            "Ollama embedding request failed: {}",
            status.as_u16()
        ));
    }
    let parsed: OllamaEmbeddingResponse =
        serde_json::from_str(&body).context("Invalid Ollama embedding response")?;
    Ok(parsed.embeddings)
}

fn embedding_client() -> Result<reqwest::Client> {
    // Embeddings share transport state, but their API keys remain scoped to each request.
    oxideterm_network_proxy::application_http_client()
        .context("failed to acquire application embedding client")
}

#[derive(Deserialize)]
struct OpenAiEmbeddingResponse {
    data: Vec<OpenAiEmbeddingItem>,
}

#[derive(Deserialize)]
struct OpenAiEmbeddingItem {
    index: usize,
    embedding: Vec<f32>,
}

#[derive(Deserialize)]
struct OllamaEmbeddingResponse {
    embeddings: Vec<Vec<f32>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedding_boundary_redacts_chunks_and_queries_before_provider_dispatch() {
        let raw_secret = "sk-proj-abcdefghijklmnopqrstuvwxyz123456";
        let sanitized_chunks =
            sanitize_embedding_texts(vec![format!("Document API_KEY={raw_secret}")]);
        let sanitized_query =
            sanitize_embedding_query(&format!("Find Authorization: Bearer {raw_secret}"));

        assert_eq!(sanitized_chunks.len(), 1);
        assert!(!sanitized_chunks[0].contains(raw_secret));
        assert!(!sanitized_query[0].contains(raw_secret));
        assert!(sanitized_chunks[0].contains("Document API_KEY=[REDACTED]"));
        assert!(sanitized_query[0].contains("Find Authorization: Bearer [REDACTED]"));
    }

    #[tokio::test]
    async fn embedding_errors_do_not_include_raw_content() {
        let raw_secret = "sk-proj-abcdefghijklmnopqrstuvwxyz123456";
        let provider = AiProviderView {
            id: "unsupported".to_string(),
            provider_type: "unsupported".to_string(),
            name: "Unsupported".to_string(),
            base_url: "https://example.invalid".to_string(),
            models: Vec::new(),
            enabled: true,
            custom: true,
        };

        let error = embed_texts(
            &provider,
            None,
            "test-model",
            vec![format!("password={raw_secret}")],
        )
        .await
        .expect_err("unsupported providers must fail");
        let debug = format!("{error:?}");
        assert!(!debug.contains(raw_secret));
        assert!(!error.to_string().contains(raw_secret));
    }
}
