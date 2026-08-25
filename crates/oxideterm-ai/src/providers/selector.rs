use std::time::Duration;

use zeroize::Zeroizing;

use super::discovery_http::fetch_openai_compatible_json;
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

pub fn model_selector_display_name(active_model: Option<&str>) -> Option<String> {
    active_model
        .filter(|model| !model.trim().is_empty())
        .map(|model| model.rsplit('/').next().unwrap_or(model).to_string())
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

    let Ok(builder) = oxideterm_network_proxy::application_http_client_builder() else {
        return false;
    };
    let Ok(client) = builder.timeout(MODEL_SELECTOR_ONLINE_TIMEOUT).build() else {
        return false;
    };
    client
        .get(format!("{base}{endpoint}"))
        .send()
        .await
        .map(|response| response.status().is_success())
        .unwrap_or(false)
}

pub async fn check_openai_compatible_model_selector_provider_online(
    base_url: &str,
    api_key: Option<Zeroizing<String>>,
) -> bool {
    let Ok(builder) = oxideterm_network_proxy::application_http_client_builder() else {
        return false;
    };
    let Ok(client) = builder.timeout(MODEL_SELECTOR_ONLINE_TIMEOUT).build() else {
        return false;
    };
    // Keep selector readiness aligned with model discovery: both use the same optional
    // authorization header, version fallback, and JSON response validation.
    fetch_openai_compatible_json(
        &client,
        base_url,
        "/models",
        api_key.as_ref(),
        "OpenAI-compatible model selector probe",
    )
    .await
    .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    use zeroize::Zeroizing;

    #[tokio::test]
    async fn openai_compatible_probe_uses_optional_key_and_version_fallback() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let request_lines = Arc::new(Mutex::new(Vec::new()));
        let server_request_lines = request_lines.clone();
        let server = tokio::spawn(async move {
            for _ in 0..4 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = vec![0_u8; 4096];
                let length = socket.read(&mut request).await.unwrap();
                let request = String::from_utf8_lossy(&request[..length]);
                let request_line = request.lines().next().unwrap_or_default().to_string();
                server_request_lines
                    .lock()
                    .unwrap()
                    .push(request_line.clone());
                let authorized = request
                    .to_ascii_lowercase()
                    .contains("authorization: bearer selector-test-key");
                let (status, body) = if request_line.contains(" /models ") {
                    ("404 Not Found", "{}")
                } else if request_line.contains(" /v1/models ") && authorized {
                    ("200 OK", r#"{"data":[{"id":"model-a"}]}"#)
                } else {
                    ("401 Unauthorized", "{}")
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            }
        });

        assert!(
            check_openai_compatible_model_selector_provider_online(
                &base_url,
                Some(Zeroizing::new("selector-test-key".to_string())),
            )
            .await
        );
        assert!(!check_openai_compatible_model_selector_provider_online(&base_url, None).await);
        server.await.unwrap();
        assert_eq!(
            request_lines.lock().unwrap().as_slice(),
            [
                "GET /models HTTP/1.1",
                "GET /v1/models HTTP/1.1",
                "GET /models HTTP/1.1",
                "GET /v1/models HTTP/1.1",
            ]
        );
    }
}
