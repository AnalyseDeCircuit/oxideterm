use serde_json::{Map, Value};
use zeroize::Zeroizing;

use crate::providers::{new_provider_from_template, provider_id, provider_string, update_provider};
use crate::{AiProviderTemplate, AiProviderView, ProviderModelRefresh};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiProviderRefreshKeyPolicy {
    NoKey,
    OptionalStoredKey,
    RequiredStoredKey,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiProviderKeyDisplayState {
    Keyless,
    Stored,
    Missing,
}

impl AiProviderKeyDisplayState {
    pub fn shows_key_control(self) -> bool {
        !matches!(self, Self::Keyless)
    }

    pub fn has_usable_key(self) -> bool {
        matches!(self, Self::Keyless | Self::Stored)
    }
}

pub fn provider_chat_requires_key(provider_type: &str) -> bool {
    // Tauri chat execution allows OpenAI-compatible endpoints to be keyless
    // so local LM Studio / gateway providers can work without credentials.
    !matches!(provider_type, "ollama" | "openai_compatible")
}

pub fn provider_key_display_state(
    provider_type: &str,
    stored_key_present: bool,
) -> AiProviderKeyDisplayState {
    // Tauri settings hides ProviderKeyInput only for Ollama. OpenAI-compatible
    // providers may run keyless, but still expose an optional key field.
    if provider_type == "ollama" {
        AiProviderKeyDisplayState::Keyless
    } else if stored_key_present {
        AiProviderKeyDisplayState::Stored
    } else {
        AiProviderKeyDisplayState::Missing
    }
}

pub fn take_provider_key_secret(draft: &mut String) -> Option<Zeroizing<String>> {
    if draft.trim().is_empty() {
        return None;
    }
    Some(Zeroizing::new(std::mem::take(draft)))
}

pub fn provider_refresh_key_policy(provider_type: &str) -> AiProviderRefreshKeyPolicy {
    match provider_type {
        "ollama" => AiProviderRefreshKeyPolicy::NoKey,
        // Match Tauri: OpenAI-compatible providers may be local or gateway
        // endpoints, so refreshing models may proceed without a stored key.
        "openai_compatible" => AiProviderRefreshKeyPolicy::OptionalStoredKey,
        _ => AiProviderRefreshKeyPolicy::RequiredStoredKey,
    }
}

pub fn set_provider_default_model(providers: &mut [Value], index: usize, model: String) {
    update_provider(providers, index, |provider| {
        provider.insert("defaultModel".to_string(), serde_json::json!(model));
    });
}

pub fn set_active_provider_selection(
    active_provider_id: &mut Option<String>,
    active_model: &mut Option<String>,
    provider: &AiProviderView,
) {
    *active_provider_id = Some(provider.id.clone());
    if !provider.default_model.trim().is_empty() {
        *active_model = Some(provider.default_model.clone());
    }
}

pub fn add_provider_from_template(
    providers: &mut Vec<Value>,
    active_provider_id: &mut Option<String>,
    active_model: &mut Option<String>,
    template: AiProviderTemplate,
    id: String,
    label: String,
    now_ms: u128,
) {
    let value = new_provider_from_template(template, id, label, now_ms);
    let default_model = provider_string(&value, "defaultModel");
    providers.push(value);
    if active_provider_id.is_none() {
        select_first_provider(providers, active_provider_id, active_model);
    } else if active_model
        .as_ref()
        .is_none_or(|model| model.trim().is_empty())
    {
        *active_model = default_model.filter(|model| !model.trim().is_empty());
    }
}

pub fn remove_provider_at(
    providers: &mut Vec<Value>,
    active_provider_id: &mut Option<String>,
    active_model: &mut Option<String>,
    index: usize,
) -> Option<String> {
    if index >= providers.len() {
        return None;
    }
    let removed = provider_id(&providers[index]);
    providers.remove(index);
    if active_provider_id.as_deref() == removed.as_deref() {
        select_first_provider(providers, active_provider_id, active_model);
    }
    removed
}

pub fn remove_provider_at_with_scoped_settings(
    providers: &mut Vec<Value>,
    active_provider_id: &mut Option<String>,
    active_model: &mut Option<String>,
    reasoning_provider_overrides: &mut Map<String, Value>,
    reasoning_model_overrides: &mut Map<String, Value>,
    user_context_windows: &mut Map<String, Value>,
    model_max_response_tokens: &mut Map<String, Value>,
    index: usize,
) -> Option<String> {
    let removed = remove_provider_at(providers, active_provider_id, active_model, index)?;
    reasoning_provider_overrides.remove(&removed);
    reasoning_model_overrides.remove(&removed);
    user_context_windows.remove(&removed);
    model_max_response_tokens.remove(&removed);
    Some(removed)
}

pub fn select_provider_model(
    providers: &mut [Value],
    active_provider_id: &mut Option<String>,
    active_model: &mut Option<String>,
    provider_id_value: &str,
    model: String,
) {
    *active_provider_id = Some(provider_id_value.to_string());
    *active_model = Some(model.clone());
    if let Some((index, _)) = providers
        .iter()
        .enumerate()
        .find(|(_, provider)| provider_id(provider).as_deref() == Some(provider_id_value))
    {
        set_provider_default_model(providers, index, model);
    }
}

pub fn apply_provider_model_refresh(
    providers: &mut [Value],
    model_context_windows: &mut Map<String, Value>,
    index: usize,
    provider_id_value: &str,
    refresh: ProviderModelRefresh,
) -> bool {
    let current_provider_id = providers.get(index).and_then(provider_id);
    if current_provider_id.as_deref() != Some(provider_id_value) {
        return false;
    }

    update_provider(providers, index, |provider| {
        provider.insert("models".to_string(), serde_json::json!(refresh.models));
    });

    if !refresh.context_windows.is_empty() {
        let mut provider_windows = model_context_windows
            .get(provider_id_value)
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        for (model, tokens) in refresh.context_windows {
            provider_windows.insert(model, serde_json::json!(tokens));
        }
        model_context_windows.insert(
            provider_id_value.to_string(),
            Value::Object(provider_windows),
        );
    }

    true
}

pub fn model_max_response_tokens(
    max_response_tokens: &Map<String, Value>,
    provider_id_value: &str,
    model: &str,
) -> Option<i64> {
    max_response_tokens
        .get(provider_id_value)
        .and_then(|value| value.get(model))
        .and_then(Value::as_i64)
        .or_else(|| max_response_tokens.get(model).and_then(Value::as_i64))
}

fn select_first_provider(
    providers: &[Value],
    active_provider_id: &mut Option<String>,
    active_model: &mut Option<String>,
) {
    let first = providers.first();
    *active_provider_id = first.and_then(provider_id);
    *active_model = first
        .and_then(|provider| provider_string(provider, "defaultModel"))
        .filter(|model| !model.trim().is_empty());
}
