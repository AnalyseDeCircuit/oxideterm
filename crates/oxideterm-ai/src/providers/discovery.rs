use std::{collections::HashMap, time::Duration};

use anyhow::{Context, Result, anyhow};
use futures_util::future::join_all;
use serde_json::Value;
use zeroize::Zeroizing;

use crate::{
    AiProviderView, ContextWindowSource, DEFAULT_CONTEXT_WINDOW, ProviderModelRefresh,
    model_context_window_info,
};

use super::discovery_http::fetch_provider_models_payload;
pub(crate) use super::discovery_http::{
    ANTHROPIC_VERSION, api_key_required_ref, looks_like_html_response,
    openai_compatible_candidates, parse_provider_json, url_encode_component,
};
pub(crate) use super::discovery_models::{parse_provider_context_windows, parse_provider_models};

const MODEL_REFRESH_TIMEOUT: Duration = Duration::from_secs(20);
const OLLAMA_SHOW_TIMEOUT: Duration = Duration::from_secs(2);
const OLLAMA_MAX_WILD_MODELS_QUERY: usize = 20;

pub async fn fetch_provider_models(
    provider: AiProviderView,
    api_key: Option<Zeroizing<String>>,
) -> Result<ProviderModelRefresh> {
    let client = oxideterm_network_proxy::application_http_client_builder()
        .context("failed to apply application proxy to model discovery client")?
        .timeout(MODEL_REFRESH_TIMEOUT)
        .build()
        .context("failed to create AI model refresh client")?;
    let provider_type = provider.provider_type.as_str();
    let payload = fetch_provider_models_payload(&client, &provider, api_key.as_ref())
        .await
        .with_context(|| format!("failed to refresh models for {}", provider.name))?;
    let models = parse_provider_models(provider_type, &payload);
    if models.is_empty() {
        return Err(anyhow!("model refresh returned no models"));
    }
    let mut context_windows = parse_provider_context_windows(provider_type, &payload);
    if provider_type == "ollama" {
        augment_ollama_context_windows(&client, &provider.base_url, &models, &mut context_windows)
            .await;
    }
    Ok(ProviderModelRefresh {
        models,
        context_windows,
    })
}

async fn augment_ollama_context_windows(
    client: &reqwest::Client,
    base_url: &str,
    models: &[String],
    context_windows: &mut HashMap<String, i64>,
) {
    let mut wild_models = Vec::new();
    for model in models {
        if let Some(context_window) = ollama_static_context_window(model) {
            context_windows.insert(model.clone(), context_window);
        } else {
            wild_models.push(model.clone());
        }
    }

    let show_url = format!("{}/api/show", base_url.trim().trim_end_matches('/'));
    let lookups = wild_models
        .into_iter()
        .take(OLLAMA_MAX_WILD_MODELS_QUERY)
        .map(|model| fetch_ollama_show_context_window(client, show_url.clone(), model));
    for result in join_all(lookups).await.into_iter().flatten() {
        context_windows.insert(result.0, result.1);
    }
}

fn ollama_static_context_window(model: &str) -> Option<i64> {
    let empty = serde_json::Map::new();
    let info = model_context_window_info(model, &empty, None, &empty);
    (info.source != ContextWindowSource::Default && info.value != DEFAULT_CONTEXT_WINDOW)
        .then_some(info.value)
}

async fn fetch_ollama_show_context_window(
    client: &reqwest::Client,
    show_url: String,
    model: String,
) -> Option<(String, i64)> {
    // Tauri treats /api/show enrichment as best-effort; failed or slow
    // lookups must not fail model refresh.
    let request = client
        .post(show_url)
        .json(&serde_json::json!({ "name": model }));
    let response = tokio::time::timeout(OLLAMA_SHOW_TIMEOUT, request.send())
        .await
        .ok()?
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let body = response.text().await.ok()?;
    let payload: Value = serde_json::from_str(&body).ok()?;
    ollama_show_context_window(&payload).map(|context_window| (model, context_window))
}

pub(crate) fn ollama_show_context_window(payload: &Value) -> Option<i64> {
    payload
        .get("model_info")
        .and_then(|info| info.get("general.context_length"))
        .or_else(|| {
            payload
                .get("model_info")
                .and_then(|info| info.get("context_length"))
        })
        .or_else(|| {
            payload
                .get("parameters")
                .and_then(|params| params.get("num_ctx"))
        })
        .and_then(Value::as_i64)
        .filter(|context_window| *context_window > 0)
}
