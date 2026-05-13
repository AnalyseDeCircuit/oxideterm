use crate::{AiProviderTemplate, AiProviderView};

pub const AI_PROVIDER_TEMPLATES: &[AiProviderTemplate] = &[
    AiProviderTemplate {
        provider_type: "openai_compatible",
        label_key: "settings_view.ai.provider_template_openai_compatible",
        base_url: "https://",
        default_model: "",
    },
    AiProviderTemplate {
        provider_type: "deepseek",
        label_key: "settings_view.ai.provider_template_deepseek",
        base_url: "https://api.deepseek.com",
        default_model: "deepseek-v4-flash",
    },
    AiProviderTemplate {
        provider_type: "openai",
        label_key: "settings_view.ai.provider_template_openai",
        base_url: "https://api.openai.com/v1",
        default_model: "gpt-4o-mini",
    },
    AiProviderTemplate {
        provider_type: "anthropic",
        label_key: "settings_view.ai.provider_template_anthropic",
        base_url: "https://api.anthropic.com",
        default_model: "claude-sonnet-4-20250514",
    },
    AiProviderTemplate {
        provider_type: "gemini",
        label_key: "settings_view.ai.provider_template_gemini",
        base_url: "https://generativelanguage.googleapis.com/v1beta",
        default_model: "gemini-2.0-flash",
    },
    AiProviderTemplate {
        provider_type: "ollama",
        label_key: "settings_view.ai.provider_template_ollama",
        base_url: "http://localhost:11434",
        default_model: "",
    },
];

pub fn provider_template_by_type(provider_type: &str) -> AiProviderTemplate {
    AI_PROVIDER_TEMPLATES
        .iter()
        .copied()
        .find(|template| template.provider_type == provider_type)
        .unwrap_or(AI_PROVIDER_TEMPLATES[0])
}

pub fn provider_views(providers: &[serde_json::Value]) -> Vec<AiProviderView> {
    providers.iter().filter_map(provider_view).collect()
}

pub fn provider_view(value: &serde_json::Value) -> Option<AiProviderView> {
    let id = provider_id(value)?;
    let provider_type =
        provider_string(value, "type").unwrap_or_else(|| "openai_compatible".to_string());
    let default_model = provider_string(value, "defaultModel").unwrap_or_default();
    Some(AiProviderView {
        custom: id.starts_with("custom-"),
        id,
        provider_type,
        name: provider_string(value, "name").unwrap_or_else(|| "Provider".to_string()),
        base_url: provider_string(value, "baseUrl").unwrap_or_default(),
        default_model,
        models: value
            .get("models")
            .and_then(|models| models.as_array())
            .map(|models| {
                models
                    .iter()
                    .filter_map(|model| model.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        enabled: value
            .get("enabled")
            .and_then(|enabled| enabled.as_bool())
            .unwrap_or(true),
    })
}

pub fn provider_id(value: &serde_json::Value) -> Option<String> {
    provider_string(value, "id")
}

pub fn provider_string(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::to_string)
}

pub fn update_provider(
    providers: &mut [serde_json::Value],
    index: usize,
    update: impl FnOnce(&mut serde_json::Map<String, serde_json::Value>),
) {
    let Some(value) = providers.get_mut(index) else {
        return;
    };
    if let Some(object) = value.as_object_mut() {
        update(object);
    }
}

pub fn active_provider_view<'a>(
    providers: &'a [AiProviderView],
    active_id: Option<&str>,
) -> Option<&'a AiProviderView> {
    providers
        .iter()
        .find(|provider| Some(provider.id.as_str()) == active_id)
}

pub fn active_model_or_provider_default(
    active_model: Option<&str>,
    provider: &AiProviderView,
) -> Option<String> {
    active_model
        .filter(|model| !model.trim().is_empty())
        .map(str::to_string)
        .or_else(|| {
            (!provider.default_model.trim().is_empty()).then(|| provider.default_model.clone())
        })
}

pub fn generated_provider_id(provider_type: &str, now_ms: u128) -> String {
    format!("custom-{provider_type}-{now_ms}")
}

pub fn new_provider_from_template(
    template: AiProviderTemplate,
    id: String,
    name: String,
    now_ms: u128,
) -> serde_json::Value {
    let mut models = Vec::new();
    if !template.default_model.is_empty() {
        models.push(template.default_model.to_string());
    }

    serde_json::json!({
        "id": id,
        "type": template.provider_type,
        "name": name,
        "baseUrl": template.base_url,
        "defaultModel": template.default_model,
        "models": models,
        "enabled": true,
        "createdAt": now_ms,
    })
}

pub fn first_provider_default_model(providers: &[serde_json::Value]) -> Option<String> {
    providers
        .first()
        .and_then(|provider| provider_string(provider, "defaultModel"))
        .filter(|model| !model.trim().is_empty())
}
