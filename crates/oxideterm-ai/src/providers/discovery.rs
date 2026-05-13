use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use zeroize::Zeroizing;

use crate::{AiProviderView, ProviderModelRefresh};

use super::discovery_http::fetch_provider_models_payload;
pub(crate) use super::discovery_http::{
    ANTHROPIC_VERSION, api_key_required_ref, looks_like_html_response,
    openai_compatible_candidates, parse_provider_json, url_encode_component,
};
pub(crate) use super::discovery_models::{parse_provider_context_windows, parse_provider_models};

const MODEL_REFRESH_TIMEOUT: Duration = Duration::from_secs(20);

pub async fn fetch_provider_models(
    provider: AiProviderView,
    api_key: Option<Zeroizing<String>>,
) -> Result<ProviderModelRefresh> {
    let client = reqwest::Client::builder()
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
    let context_windows = parse_provider_context_windows(provider_type, &payload);
    Ok(ProviderModelRefresh {
        models,
        context_windows,
    })
}
