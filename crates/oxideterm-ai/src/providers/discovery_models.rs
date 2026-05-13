use std::collections::HashMap;

use serde_json::Value;

pub(crate) fn parse_provider_models(provider_type: &str, payload: &Value) -> Vec<String> {
    let models = match provider_type {
        "gemini" => payload
            .get("models")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|model| {
                model
                    .get("supportedGenerationMethods")
                    .and_then(Value::as_array)
                    .is_some_and(|methods| {
                        methods
                            .iter()
                            .any(|method| method.as_str() == Some("generateContent"))
                    })
            })
            .filter_map(|model| {
                model
                    .get("name")
                    .and_then(Value::as_str)
                    .map(|name| name.strip_prefix("models/").unwrap_or(name).to_string())
            })
            .collect::<Vec<_>>(),
        "ollama" => payload
            .get("models")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|model| {
                model
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .collect::<Vec<_>>(),
        "openai_compatible" | "deepseek" => openai_compatible_model_values(payload)
            .filter_map(|model| {
                model
                    .get("id")
                    .or_else(|| model.get("key"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .collect::<Vec<_>>(),
        "openai" => payload
            .get("data")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|model| model.get("id").and_then(Value::as_str).map(str::to_string))
            .filter(|id| {
                id.starts_with("gpt-")
                    || id.starts_with("chatgpt-")
                    || (id.starts_with('o')
                        && id.chars().nth(1).is_some_and(|ch| ch.is_ascii_digit()))
                    || id.contains("turbo")
                    || id.contains("chat")
            })
            .collect::<Vec<_>>(),
        "anthropic" => payload
            .get("data")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|model| model.get("id").and_then(Value::as_str).map(str::to_string))
            .filter(|id| id.starts_with("claude-"))
            .collect::<Vec<_>>(),
        _ => payload
            .get("data")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|model| model.get("id").and_then(Value::as_str).map(str::to_string))
            .collect::<Vec<_>>(),
    };
    let mut models = dedupe_non_empty(models);
    if models.is_empty() && provider_type == "openai" {
        models = payload
            .get("data")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|model| model.get("id").and_then(Value::as_str).map(str::to_string))
            .collect();
        models = dedupe_non_empty(models);
    }
    models.sort();
    models
}

fn openai_compatible_model_values(payload: &Value) -> impl Iterator<Item = &Value> {
    payload
        .get("data")
        .and_then(Value::as_array)
        .or_else(|| payload.get("models").and_then(Value::as_array))
        .into_iter()
        .flatten()
}

pub(crate) fn parse_provider_context_windows(
    provider_type: &str,
    payload: &Value,
) -> HashMap<String, i64> {
    let mut result = HashMap::new();
    match provider_type {
        "gemini" => {
            for model in payload
                .get("models")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let Some(id) = model
                    .get("name")
                    .and_then(Value::as_str)
                    .map(|name| name.strip_prefix("models/").unwrap_or(name))
                else {
                    continue;
                };
                if let Some(ctx) = model.get("inputTokenLimit").and_then(Value::as_i64) {
                    result.insert(id.to_string(), ctx);
                }
            }
        }
        "anthropic" => {
            for model in payload
                .get("data")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let Some(id) = model.get("id").and_then(Value::as_str) else {
                    continue;
                };
                if let Some(ctx) = model
                    .get("context_window")
                    .or_else(|| model.get("input_token_limit"))
                    .and_then(Value::as_i64)
                {
                    result.insert(id.to_string(), ctx);
                }
            }
        }
        _ => {
            for model in openai_compatible_model_values(payload) {
                let Some(id) = model
                    .get("id")
                    .or_else(|| model.get("key"))
                    .and_then(Value::as_str)
                else {
                    continue;
                };
                if let Some(ctx) = model
                    .get("context_window")
                    .or_else(|| model.get("context_length"))
                    .and_then(Value::as_i64)
                {
                    result.insert(id.to_string(), ctx);
                }
            }
        }
    }
    result
}

fn dedupe_non_empty(models: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    models
        .into_iter()
        .filter_map(|model| {
            let model = model.trim().to_string();
            (!model.is_empty() && seen.insert(model.clone())).then_some(model)
        })
        .collect()
}
