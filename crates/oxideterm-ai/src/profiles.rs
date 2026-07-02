use crate::AiToolUsePolicy;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum AiExecutionBackend {
    #[default]
    Provider,
    Acp,
}

pub fn resolve_ai_reasoning_effort(
    base_reasoning_effort: Option<&str>,
    reasoning_provider_overrides: &serde_json::Map<String, Value>,
    reasoning_model_overrides: &serde_json::Map<String, Value>,
    provider_id: Option<&str>,
    model_id: Option<&str>,
) -> String {
    if let Some(model_override) = provider_id
        .zip(model_id)
        .and_then(|(provider_id, model_id)| {
            reasoning_model_overrides
                .get(provider_id)
                .and_then(Value::as_object)
                .and_then(|models| models.get(model_id))
                .and_then(Value::as_str)
        })
        .filter(|value| !value.trim().is_empty())
    {
        return normalize_reasoning_effort(model_override).to_string();
    }

    if let Some(provider_override) = provider_id
        .and_then(|provider_id| reasoning_provider_overrides.get(provider_id))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        return normalize_reasoning_effort(provider_override).to_string();
    }

    normalize_reasoning_effort(base_reasoning_effort.unwrap_or("auto")).to_string()
}

fn normalize_reasoning_effort(value: &str) -> &'static str {
    match value {
        "none" | "off" => "off",
        "minimal" | "low" => "low",
        "medium" => "medium",
        "high" => "high",
        "xhigh" | "max" => "max",
        _ => "auto",
    }
}

pub fn tool_policy_from_parts(
    enabled: bool,
    auto_approve_tools: impl IntoIterator<Item = (String, bool)>,
    disabled_tools: Vec<String>,
    max_rounds: Option<i64>,
    max_calls_per_round: Option<i64>,
) -> AiToolUsePolicy {
    AiToolUsePolicy {
        enabled,
        auto_approve_tools: HashMap::from_iter(auto_approve_tools),
        disabled_tools,
        max_rounds,
        max_calls_per_round,
    }
}
