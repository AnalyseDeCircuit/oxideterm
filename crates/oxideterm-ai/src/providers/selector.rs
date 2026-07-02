use std::time::Duration;

use crate::{AiProviderView, ModelSelectorProviderGroup, ModelSelectorProviderProbe};

const MODEL_SELECTOR_ONLINE_TIMEOUT: Duration = Duration::from_secs(3);

pub fn is_local_provider_url(base_url: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(base_url) else {
        return false;
    };
    let Some(host) = url.host_str().map(str::to_ascii_lowercase) else {
        return false;
    };
    if host == "localhost"
        || host == "127.0.0.1"
        || host == "::1"
        || host == "[::1]"
        || host.ends_with(".local")
    {
        return true;
    }
    if host.starts_with("192.168.") || host.starts_with("10.") {
        return true;
    }
    if let Some(octet) = host
        .strip_prefix("172.")
        .and_then(|rest| rest.split('.').next())
        .and_then(|octet| octet.parse::<u8>().ok())
    {
        return (16..=31).contains(&octet);
    }
    false
}

pub fn resolve_model_selector_provider_probe(
    provider: &AiProviderView,
) -> ModelSelectorProviderProbe {
    if !provider.enabled {
        return ModelSelectorProviderProbe::Disabled;
    }
    if provider.provider_type == "acp" {
        return ModelSelectorProviderProbe::ImplicitKey { endpoint: None };
    }
    if provider.provider_type == "ollama" {
        return ModelSelectorProviderProbe::ImplicitKey {
            endpoint: Some("/api/tags"),
        };
    }
    if provider.provider_type == "openai_compatible" && is_local_provider_url(&provider.base_url) {
        return ModelSelectorProviderProbe::ImplicitKey {
            endpoint: Some("/models"),
        };
    }
    ModelSelectorProviderProbe::StoredKey
}

pub fn model_selector_display_name(
    active_provider: Option<&AiProviderView>,
    active_model: Option<&str>,
) -> String {
    let Some(provider) = active_provider else {
        return "OxideSens".to_string();
    };
    let model = active_model
        .filter(|model| !model.trim().is_empty())
        .unwrap_or(provider.default_model.as_str());
    if model.trim().is_empty() {
        provider.name.clone()
    } else {
        format!(
            "{}/{}",
            provider.name,
            model.rsplit('/').next().unwrap_or(model)
        )
    }
}

pub fn model_selector_truncated_label(label: &str) -> String {
    if label.chars().count() > 24 {
        let truncated = label.chars().take(22).collect::<String>();
        format!("{truncated}...")
    } else {
        label.to_string()
    }
}

pub fn model_selector_visible_provider_groups(
    providers: &[AiProviderView],
    query: &str,
) -> Vec<ModelSelectorProviderGroup> {
    let normalized = query.trim().to_ascii_lowercase();
    let searching = !normalized.is_empty();
    providers
        .iter()
        .filter(|provider| provider.enabled)
        .filter_map(|provider| {
            let provider_matches = provider.name.to_ascii_lowercase().contains(&normalized);
            let visible_models = if searching {
                provider
                    .models
                    .iter()
                    .filter(|model| {
                        provider_matches || model.to_ascii_lowercase().contains(&normalized)
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            } else {
                provider.models.clone()
            };
            (!searching || !visible_models.is_empty()).then(|| ModelSelectorProviderGroup {
                provider: provider.clone(),
                visible_models,
            })
        })
        .collect()
}

pub async fn check_model_selector_provider_online(base_url: &str, endpoint: &str) -> bool {
    let base = base_url.trim().trim_end_matches('/');
    if base.is_empty() || endpoint.trim().is_empty() {
        return false;
    }

    let Ok(client) = reqwest::Client::builder()
        .timeout(MODEL_SELECTOR_ONLINE_TIMEOUT)
        .build()
    else {
        return false;
    };
    client
        .get(format!("{base}{endpoint}"))
        .send()
        .await
        .map(|response| response.status().is_success())
        .unwrap_or(false)
}
