use crate::AiToolUsePolicy;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum AiExecutionBackend {
    #[default]
    Provider,
    Acp,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedAiExecutionProfile {
    pub profile_id: Option<String>,
    pub backend: AiExecutionBackend,
    pub provider_id: Option<String>,
    pub acp_agent_id: Option<String>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub tool_policy: AiToolUsePolicy,
    pub include_runtime_chips: bool,
    pub include_memory: bool,
    pub include_rag: bool,
}

impl Default for ResolvedAiExecutionProfile {
    fn default() -> Self {
        Self {
            profile_id: None,
            backend: AiExecutionBackend::Provider,
            provider_id: None,
            acp_agent_id: None,
            model: None,
            reasoning_effort: None,
            tool_policy: AiToolUsePolicy::default(),
            include_runtime_chips: true,
            include_memory: true,
            include_rag: true,
        }
    }
}

pub fn resolve_ai_execution_profile(
    config: &Value,
    requested_profile_id: Option<&str>,
    base_provider_id: Option<&str>,
    base_model: Option<&str>,
    base_reasoning_effort: Option<&str>,
    base_tool_policy: AiToolUsePolicy,
) -> ResolvedAiExecutionProfile {
    let profile = resolve_profile_value(config, requested_profile_id);
    let mut resolved = ResolvedAiExecutionProfile {
        profile_id: profile
            .and_then(|profile| string_field(profile, "id"))
            .or_else(|| requested_profile_id.map(str::to_string)),
        backend: AiExecutionBackend::Provider,
        provider_id: base_provider_id.map(str::to_string),
        acp_agent_id: None,
        model: base_model.map(str::to_string),
        reasoning_effort: base_reasoning_effort.map(str::to_string),
        tool_policy: base_tool_policy,
        include_runtime_chips: true,
        include_memory: true,
        include_rag: true,
    };

    let Some(profile) = profile else {
        return resolved;
    };

    if profile.get("backend").and_then(Value::as_str) == Some("acp") {
        resolved.backend = AiExecutionBackend::Acp;
        resolved.acp_agent_id = string_field(profile, "acpAgentId");
        resolved.provider_id = None;
        resolved.model = None;
    }
    if resolved.backend == AiExecutionBackend::Provider {
        if let Some(provider_id) = string_field(profile, "providerId") {
            resolved.provider_id = Some(provider_id);
        }
        if let Some(model) = string_field(profile, "model") {
            resolved.model = Some(model);
        }
    }
    if let Some(reasoning_effort) = string_field(profile, "reasoningEffort") {
        resolved.reasoning_effort = Some(reasoning_effort);
    }
    if let Some(tool_use) = profile.get("toolUse").and_then(Value::as_object) {
        resolved.tool_policy = merge_tool_policy(resolved.tool_policy, tool_use);
    }
    if let Some(context) = profile.get("context").and_then(Value::as_object) {
        if let Some(include_runtime_chips) =
            context.get("includeRuntimeChips").and_then(Value::as_bool)
        {
            resolved.include_runtime_chips = include_runtime_chips;
        }
        if let Some(include_memory) = context.get("includeMemory").and_then(Value::as_bool) {
            resolved.include_memory = include_memory;
        }
        if let Some(include_rag) = context.get("includeRag").and_then(Value::as_bool) {
            resolved.include_rag = include_rag;
        }
    }

    resolved
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

fn resolve_profile_value<'a>(
    config: &'a Value,
    requested_profile_id: Option<&str>,
) -> Option<&'a Value> {
    let profiles = config.get("profiles").and_then(Value::as_array)?;
    if profiles.is_empty() {
        return None;
    }
    requested_profile_id
        .and_then(|id| profile_by_id(profiles, id))
        .or_else(|| {
            config
                .get("defaultProfileId")
                .and_then(Value::as_str)
                .and_then(|id| profile_by_id(profiles, id))
        })
        .or_else(|| profiles.first())
}

fn profile_by_id<'a>(profiles: &'a [Value], id: &str) -> Option<&'a Value> {
    profiles
        .iter()
        .find(|profile| profile.get("id").and_then(Value::as_str) == Some(id))
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
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

fn merge_tool_policy(
    mut base: AiToolUsePolicy,
    profile_tool_use: &serde_json::Map<String, Value>,
) -> AiToolUsePolicy {
    // Source parity: Tauri `applyExecutionProfileToAiSettings` shallow-merges
    // profile.toolUse over settings.toolUse, then merges autoApproveTools and
    // lets profile.disabledTools replace the base list only when present.
    if let Some(enabled) = profile_tool_use.get("enabled").and_then(Value::as_bool) {
        base.enabled = enabled;
    }
    if let Some(max_rounds) = profile_tool_use.get("maxRounds").and_then(Value::as_i64) {
        base.max_rounds = Some(max_rounds);
    }
    if let Some(max_calls_per_round) = profile_tool_use
        .get("maxCallsPerRound")
        .and_then(Value::as_i64)
    {
        base.max_calls_per_round = Some(max_calls_per_round);
    }
    if let Some(auto_approve) = profile_tool_use
        .get("autoApproveTools")
        .and_then(Value::as_object)
    {
        for (key, value) in auto_approve {
            if let Some(enabled) = value.as_bool() {
                base.auto_approve_tools.insert(key.clone(), enabled);
            }
        }
    }
    if let Some(disabled_tools) = profile_tool_use
        .get("disabledTools")
        .and_then(Value::as_array)
    {
        base.disabled_tools = disabled_tools
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
    }
    base
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
