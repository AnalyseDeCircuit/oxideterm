// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! AI settings page model helpers.

use std::{
    collections::{HashSet, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
};

use oxideterm_ai::{
    AiProviderView, ContextWindowSource, McpAuthHeaderMode, McpTransport, ProviderModelRefresh,
    model_context_window_info, provider_views as ai_provider_views_from_values,
    update_provider as ai_update_provider_values,
};
use oxideterm_settings::PersistedSettings;

use crate::SettingsInput;

pub fn ai_provider_views(settings: &PersistedSettings) -> Vec<AiProviderView> {
    ai_provider_views_from_values(&settings.ai.providers)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiToolPolicyItem {
    pub key: Option<&'static str>,
    pub label_key: &'static str,
    pub checked: bool,
    pub locked: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiToolPolicyGroup {
    pub title_key: &'static str,
    pub description_key: &'static str,
    pub items: Vec<AiToolPolicyItem>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiProviderModelPanel {
    pub provider_index: usize,
    pub provider_id: String,
    pub provider_name: String,
    pub model_count: usize,
    pub override_count: usize,
    pub models: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiModelReasoningRow {
    pub current_value: String,
    pub label_key: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiModelContextWindowRow {
    pub has_override: bool,
    pub source: ContextWindowSource,
}

pub fn ai_tool_auto_approved_count(settings: &PersistedSettings) -> usize {
    settings
        .ai
        .tool_use
        .auto_approve_tools
        .values()
        .filter(|value| value.as_bool() == Some(true))
        .count()
}

pub fn ai_tool_auto_approve_total_count(settings: &PersistedSettings) -> usize {
    settings.ai.tool_use.auto_approve_tools.len()
}

pub fn ai_tool_policy_groups(settings: &PersistedSettings) -> Vec<AiToolPolicyGroup> {
    let auto = &settings.ai.tool_use.auto_approve_tools;
    let checked = |key: &str| auto.get(key).and_then(serde_json::Value::as_bool) == Some(true);
    vec![
        AiToolPolicyGroup {
            title_key: "settings_view.ai.tool_policy_read_title",
            description_key: "settings_view.ai.tool_policy_read_desc",
            items: vec![AiToolPolicyItem {
                key: None,
                label_key: "settings_view.ai.tool_policy_read_auto",
                checked: true,
                locked: true,
            }],
        },
        AiToolPolicyGroup {
            title_key: "settings_view.ai.tool_policy_execute_title",
            description_key: "settings_view.ai.tool_policy_execute_desc",
            items: vec![AiToolPolicyItem {
                key: Some("run_command"),
                label_key: "settings_view.ai.tool_policy_execute_run_command",
                checked: checked("run_command"),
                locked: false,
            }],
        },
        AiToolPolicyGroup {
            title_key: "settings_view.ai.tool_policy_interactive_title",
            description_key: "settings_view.ai.tool_policy_interactive_desc",
            items: vec![AiToolPolicyItem {
                key: Some("send_terminal_input"),
                label_key: "settings_view.ai.tool_policy_interactive_send_input",
                checked: checked("send_terminal_input"),
                locked: false,
            }],
        },
        AiToolPolicyGroup {
            title_key: "settings_view.ai.tool_policy_navigation_title",
            description_key: "settings_view.ai.tool_policy_navigation_desc",
            items: vec![
                AiToolPolicyItem {
                    key: Some("connect_target"),
                    label_key: "settings_view.ai.tool_policy_connect_target",
                    checked: checked("connect_target"),
                    locked: false,
                },
                AiToolPolicyItem {
                    key: Some("open_app_surface"),
                    label_key: "settings_view.ai.tool_policy_open_surface",
                    checked: checked("open_app_surface"),
                    locked: false,
                },
            ],
        },
        AiToolPolicyGroup {
            title_key: "settings_view.ai.tool_policy_write_title",
            description_key: "settings_view.ai.tool_policy_write_desc",
            items: vec![
                AiToolPolicyItem {
                    key: Some("write_resource:settings"),
                    label_key: "settings_view.ai.tool_policy_write_settings",
                    checked: checked("write_resource:settings"),
                    locked: false,
                },
                AiToolPolicyItem {
                    key: Some("write_resource:file"),
                    label_key: "settings_view.ai.tool_policy_write_file",
                    checked: checked("write_resource:file"),
                    locked: false,
                },
                AiToolPolicyItem {
                    key: Some("transfer_resource"),
                    label_key: "settings_view.ai.tool_policy_transfer_resource",
                    checked: checked("transfer_resource"),
                    locked: false,
                },
                AiToolPolicyItem {
                    key: Some("remember_preference"),
                    label_key: "settings_view.ai.tool_policy_remember_preference",
                    checked: checked("remember_preference"),
                    locked: false,
                },
            ],
        },
    ]
}

pub fn ai_reasoning_label_key(value: &str) -> &'static str {
    match value {
        "off" | "none" => "settings_view.ai.reasoning_off",
        "low" | "minimal" => "settings_view.ai.reasoning_low",
        "medium" => "settings_view.ai.reasoning_medium",
        "high" => "settings_view.ai.reasoning_high",
        "max" | "xhigh" => "settings_view.ai.reasoning_max",
        _ => "settings_view.ai.reasoning_auto",
    }
}

pub fn ai_context_max_chars_label_key(value: i64) -> Option<&'static str> {
    match value {
        2_000 => Some("settings_view.ai.chars_2000"),
        4_000 => Some("settings_view.ai.chars_4000"),
        8_000 => Some("settings_view.ai.chars_8000"),
        16_000 => Some("settings_view.ai.chars_16000"),
        32_000 => Some("settings_view.ai.chars_32000"),
        _ => None,
    }
}

pub fn ai_context_visible_lines_label_key(value: i64) -> Option<&'static str> {
    match value {
        50 => Some("settings_view.ai.lines_50"),
        100 => Some("settings_view.ai.lines_100"),
        200 => Some("settings_view.ai.lines_200"),
        400 => Some("settings_view.ai.lines_400"),
        _ => None,
    }
}

pub fn ai_model_reasoning_panels(
    settings: &PersistedSettings,
    providers: &[AiProviderView],
) -> Vec<AiProviderModelPanel> {
    providers
        .iter()
        .enumerate()
        .filter(|(_, provider)| !provider.models.is_empty())
        .map(|(provider_index, provider)| {
            let override_count = provider
                .models
                .iter()
                .filter(|model| {
                    settings
                        .ai
                        .reasoning_model_overrides
                        .get(&provider.id)
                        .and_then(|models| models.get(model.as_str()))
                        .is_some()
                })
                .count();
            AiProviderModelPanel {
                provider_index,
                provider_id: provider.id.clone(),
                provider_name: provider.name.clone(),
                model_count: provider.models.len(),
                override_count,
                models: provider.models.clone(),
            }
        })
        .collect()
}

pub fn ai_model_context_window_panels(
    settings: &PersistedSettings,
    providers: &[AiProviderView],
) -> Vec<AiProviderModelPanel> {
    providers
        .iter()
        .enumerate()
        .filter(|(_, provider)| !provider.models.is_empty())
        .map(|(provider_index, provider)| {
            let override_count = provider
                .models
                .iter()
                .filter(|model| {
                    settings
                        .ai
                        .user_context_windows
                        .get(&provider.id)
                        .and_then(|windows| windows.get(model.as_str()))
                        .is_some()
                })
                .count();
            AiProviderModelPanel {
                provider_index,
                provider_id: provider.id.clone(),
                provider_name: provider.name.clone(),
                model_count: provider.models.len(),
                override_count,
                models: provider.models.clone(),
            }
        })
        .collect()
}

pub fn ai_model_reasoning_row(
    settings: &PersistedSettings,
    provider_id: &str,
    model: &str,
) -> AiModelReasoningRow {
    let current_value = settings
        .ai
        .reasoning_model_overrides
        .get(provider_id)
        .and_then(|models| models.get(model))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("__inherit__")
        .to_string();
    let label_key = if current_value == "__inherit__" {
        "settings_view.ai.reasoning_inherit_provider"
    } else {
        ai_reasoning_label_key(&current_value)
    };
    AiModelReasoningRow {
        current_value,
        label_key,
    }
}

pub fn ai_model_context_window_row(
    settings: &PersistedSettings,
    provider_id: &str,
    model: &str,
) -> AiModelContextWindowRow {
    let has_override = settings
        .ai
        .user_context_windows
        .get(provider_id)
        .and_then(|windows| windows.get(model))
        .is_some();
    let info = model_context_window_info(
        model,
        &settings.ai.model_context_windows,
        Some(provider_id),
        &settings.ai.user_context_windows,
    );
    AiModelContextWindowRow {
        has_override,
        source: info.source,
    }
}

pub fn ai_execution_profile_signature(
    profile: &serde_json::Value,
    default_profile_id: &str,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    // Profile cards expose the serialized profile fields plus default status.
    // Hashing that view model keeps virtual-list remeasurement deterministic.
    serde_json::to_string(profile)
        .unwrap_or_default()
        .hash(&mut hasher);
    ai_execution_profile_id(profile)
        .as_deref()
        .map(|id| id == default_profile_id)
        .unwrap_or(false)
        .hash(&mut hasher);
    hasher.finish()
}

pub fn ai_provider_model_row_signature(
    provider_id: &str,
    model: &str,
    override_value: Option<&serde_json::Value>,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    // Model override rows expose provider/model identity and the current
    // override cell, not hidden provider secrets or app-local view state.
    provider_id.hash(&mut hasher);
    model.hash(&mut hasher);
    override_value
        .map(serde_json::Value::to_string)
        .unwrap_or_default()
        .hash(&mut hasher);
    hasher.finish()
}

pub fn ai_provider_card_signature(
    provider: &AiProviderView,
    expanded: bool,
    models_expanded: bool,
    has_key: bool,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    // Provider cards expose public config, expansion state, model count, and
    // key-control visibility. Secret key material intentionally stays out.
    provider.id.hash(&mut hasher);
    provider.name.hash(&mut hasher);
    provider.provider_type.hash(&mut hasher);
    provider.enabled.hash(&mut hasher);
    provider.custom.hash(&mut hasher);
    provider.default_model.hash(&mut hasher);
    provider.base_url.hash(&mut hasher);
    provider.models.len().hash(&mut hasher);
    expanded.hash(&mut hasher);
    models_expanded.hash(&mut hasher);
    has_key.hash(&mut hasher);
    hasher.finish()
}

pub fn ai_update_provider(
    settings: &mut PersistedSettings,
    index: usize,
    update: impl FnOnce(&mut serde_json::Map<String, serde_json::Value>),
) {
    ai_update_provider_values(&mut settings.ai.providers, index, update);
}

pub fn toggle_string_set(set: &mut HashSet<String>, value: &str) {
    if !set.remove(value) {
        set.insert(value.to_string());
    }
}

pub fn current_time_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn ai_execution_profiles_array_mut(
    settings: &mut PersistedSettings,
) -> Option<&mut Vec<serde_json::Value>> {
    settings
        .ai
        .execution_profiles
        .get_mut("profiles")
        .and_then(serde_json::Value::as_array_mut)
}

pub fn ai_patch_execution_profile(
    settings: &mut PersistedSettings,
    index: usize,
    patch: impl FnOnce(&mut serde_json::Map<String, serde_json::Value>),
) {
    let Some(profile) = ai_execution_profiles_array_mut(settings)
        .and_then(|profiles| profiles.get_mut(index))
        .and_then(serde_json::Value::as_object_mut)
    else {
        return;
    };
    patch(profile);
}

pub fn ai_execution_profile_id(profile: &serde_json::Value) -> Option<String> {
    profile
        .get("id")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

fn ai_fallback_execution_profile(settings: &PersistedSettings) -> serde_json::Value {
    let now = current_time_millis();
    serde_json::json!({
        "id": "default",
        "name": "Default",
        "providerId": settings.ai.active_provider_id.clone(),
        "model": settings.ai.active_model.clone(),
        "reasoningEffort": ai_reasoning_profile_value(settings.ai.reasoning_effort),
        "toolUse": {
            "enabled": settings.ai.tool_use.enabled,
            "maxRounds": settings.ai.tool_use.max_rounds,
            "maxCallsPerRound": settings.ai.tool_use.max_calls_per_round,
            "autoApproveTools": settings.ai.tool_use.auto_approve_tools.clone(),
            "disabledTools": settings.ai.tool_use.disabled_tools.clone()
        },
        "context": {
            "includeRuntimeChips": true,
            "includeMemory": true,
            "includeRag": true
        },
        "commandPolicy": { "allow": [], "deny": [] },
        "createdAt": now,
        "updatedAt": now
    })
}

pub fn ai_execution_profiles_need_normalization(settings: &PersistedSettings) -> bool {
    let Some(profiles) = settings
        .ai
        .execution_profiles
        .get("profiles")
        .and_then(serde_json::Value::as_array)
    else {
        return true;
    };
    if profiles.is_empty() {
        return true;
    }
    let default_id = settings
        .ai
        .execution_profiles
        .get("defaultProfileId")
        .and_then(serde_json::Value::as_str);
    default_id.is_none_or(|default_id| {
        !profiles.iter().any(|profile| {
            profile.get("id").and_then(serde_json::Value::as_str) == Some(default_id)
        })
    })
}

pub fn ai_normalize_execution_profiles(settings: &mut PersistedSettings) {
    let Some(profiles) = settings
        .ai
        .execution_profiles
        .get("profiles")
        .and_then(serde_json::Value::as_array)
        .cloned()
    else {
        settings.ai.execution_profiles = serde_json::json!({
            "defaultProfileId": "default",
            "profiles": [ai_fallback_execution_profile(settings)]
        });
        return;
    };

    if profiles.is_empty() {
        settings.ai.execution_profiles = serde_json::json!({
            "defaultProfileId": "default",
            "profiles": [ai_fallback_execution_profile(settings)]
        });
        return;
    }

    let current_default = settings
        .ai
        .execution_profiles
        .get("defaultProfileId")
        .and_then(serde_json::Value::as_str);
    let default_is_valid = current_default.is_some_and(|default_id| {
        profiles.iter().any(|profile| {
            profile.get("id").and_then(serde_json::Value::as_str) == Some(default_id)
        })
    });
    if default_is_valid {
        return;
    }
    let object = settings
        .ai
        .execution_profiles
        .as_object_mut()
        .expect("execution_profiles with a profiles array must be an object");
    let next_default = profiles
        .iter()
        .find_map(ai_execution_profile_id)
        .unwrap_or_else(|| {
            // Migrated profiles can exist without ids. If we only write
            // defaultProfileId here, the next render still sees an invalid
            // default and schedules the same normalization again.
            let profile_id = "default".to_string();
            if let Some(first_profile) = object
                .get_mut("profiles")
                .and_then(serde_json::Value::as_array_mut)
                .and_then(|profiles| profiles.first_mut())
                .and_then(serde_json::Value::as_object_mut)
            {
                first_profile.insert("id".to_string(), serde_json::json!(profile_id.clone()));
            }
            profile_id
        });
    object.insert(
        "defaultProfileId".to_string(),
        serde_json::json!(next_default),
    );
}

pub fn ai_default_execution_profile(settings: &PersistedSettings) -> Option<String> {
    settings
        .ai
        .execution_profiles
        .get("defaultProfileId")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

pub fn ai_add_execution_profile(settings: &mut PersistedSettings) {
    let now = current_time_millis();
    let profile_count = settings
        .ai
        .execution_profiles
        .get("profiles")
        .and_then(serde_json::Value::as_array)
        .map(Vec::len)
        .unwrap_or_default();
    let profile_id = format!("profile-{now}");
    let profile = serde_json::json!({
        "id": profile_id,
        "name": format!("Profile {}", profile_count + 1),
        "providerId": settings.ai.active_provider_id.clone(),
        "model": settings.ai.active_model.clone(),
        "reasoningEffort": ai_reasoning_profile_value(settings.ai.reasoning_effort),
        "toolUse": {
            "enabled": settings.ai.tool_use.enabled,
            "maxRounds": settings.ai.tool_use.max_rounds,
            "maxCallsPerRound": settings.ai.tool_use.max_calls_per_round,
            "autoApproveTools": settings.ai.tool_use.auto_approve_tools.clone(),
            "disabledTools": settings.ai.tool_use.disabled_tools.clone()
        },
        "context": {
            "includeRuntimeChips": true,
            "includeMemory": true,
            "includeRag": true
        },
        "commandPolicy": { "allow": [], "deny": [] },
        "createdAt": now,
        "updatedAt": now
    });
    if let Some(profiles) = ai_execution_profiles_array_mut(settings) {
        profiles.push(profile);
    }
    if let Some(object) = settings.ai.execution_profiles.as_object_mut() {
        object.insert(
            "defaultProfileId".to_string(),
            serde_json::json!(profile_id),
        );
    }
}

pub fn ai_duplicate_execution_profile(settings: &mut PersistedSettings, index: usize) {
    let Some(source) = settings
        .ai
        .execution_profiles
        .get("profiles")
        .and_then(serde_json::Value::as_array)
        .and_then(|profiles| profiles.get(index))
        .cloned()
    else {
        return;
    };
    let now = current_time_millis();
    let mut copy = source;
    let copy_id = format!("profile-{now}");
    if let Some(object) = copy.as_object_mut() {
        let name = object
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("Profile")
            .to_string();
        object.insert("id".to_string(), serde_json::json!(copy_id.clone()));
        object.insert(
            "name".to_string(),
            serde_json::json!(format!("{name} Copy")),
        );
        object.insert("createdAt".to_string(), serde_json::json!(now));
        object.insert("updatedAt".to_string(), serde_json::json!(now));
    }
    if let Some(profiles) = ai_execution_profiles_array_mut(settings) {
        profiles.push(copy);
    }
    if let Some(object) = settings.ai.execution_profiles.as_object_mut() {
        object.insert("defaultProfileId".to_string(), serde_json::json!(copy_id));
    }
}

pub fn ai_delete_execution_profile(settings: &mut PersistedSettings, index: usize) {
    let Some(profiles) = ai_execution_profiles_array_mut(settings) else {
        return;
    };
    if profiles.len() <= 1 || index >= profiles.len() {
        return;
    }
    let removed_default = ai_default_execution_profile(settings)
        .zip(
            settings
                .ai
                .execution_profiles
                .get("profiles")
                .and_then(serde_json::Value::as_array)
                .and_then(|profiles| profiles.get(index))
                .and_then(ai_execution_profile_id),
        )
        .is_some_and(|(default_id, removed_id)| default_id == removed_id);
    if let Some(profiles) = ai_execution_profiles_array_mut(settings) {
        profiles.remove(index);
    }
    if removed_default {
        let next_id = settings
            .ai
            .execution_profiles
            .get("profiles")
            .and_then(serde_json::Value::as_array)
            .and_then(|profiles| profiles.first())
            .and_then(ai_execution_profile_id)
            .unwrap_or_else(|| "default".to_string());
        if let Some(object) = settings.ai.execution_profiles.as_object_mut() {
            object.insert("defaultProfileId".to_string(), serde_json::json!(next_id));
        }
    }
}

pub fn ai_set_default_execution_profile(settings: &mut PersistedSettings, profile_id: String) {
    if let Some(object) = settings.ai.execution_profiles.as_object_mut() {
        object.insert(
            "defaultProfileId".to_string(),
            serde_json::json!(profile_id),
        );
    }
}

pub fn ai_reasoning_profile_value(effort: oxideterm_settings::AiReasoningEffort) -> &'static str {
    match effort {
        oxideterm_settings::AiReasoningEffort::None => "off",
        oxideterm_settings::AiReasoningEffort::Minimal => "low",
        oxideterm_settings::AiReasoningEffort::Low => "low",
        oxideterm_settings::AiReasoningEffort::Medium => "medium",
        oxideterm_settings::AiReasoningEffort::High => "high",
        oxideterm_settings::AiReasoningEffort::Xhigh => "max",
        oxideterm_settings::AiReasoningEffort::Auto => "auto",
    }
}

pub fn ai_reasoning_effort_from_profile_value(
    value: &str,
) -> oxideterm_settings::AiReasoningEffort {
    match value {
        "off" => oxideterm_settings::AiReasoningEffort::None,
        "low" => oxideterm_settings::AiReasoningEffort::Low,
        "medium" => oxideterm_settings::AiReasoningEffort::Medium,
        "high" => oxideterm_settings::AiReasoningEffort::High,
        "max" => oxideterm_settings::AiReasoningEffort::Xhigh,
        _ => oxideterm_settings::AiReasoningEffort::Auto,
    }
}

pub fn set_ai_provider_reasoning_override(
    settings: &mut PersistedSettings,
    provider_id: &str,
    value: Option<&'static str>,
) {
    match value {
        Some(value) => {
            settings
                .ai
                .reasoning_provider_overrides
                .insert(provider_id.to_string(), serde_json::json!(value));
        }
        None => {
            settings.ai.reasoning_provider_overrides.remove(provider_id);
        }
    }
}

pub fn set_ai_model_reasoning_override(
    settings: &mut PersistedSettings,
    provider_id: &str,
    model: &str,
    value: Option<&'static str>,
) {
    let provider_entry = settings
        .ai
        .reasoning_model_overrides
        .entry(provider_id.to_string())
        .or_insert_with(|| serde_json::json!({}));
    let Some(provider_overrides) = provider_entry.as_object_mut() else {
        return;
    };
    match value {
        Some(value) => {
            provider_overrides.insert(model.to_string(), serde_json::json!(value));
        }
        None => {
            provider_overrides.remove(model);
        }
    }
    if provider_overrides.is_empty() {
        settings.ai.reasoning_model_overrides.remove(provider_id);
    }
}

pub fn set_ai_user_context_window(
    settings: &mut PersistedSettings,
    provider_id: &str,
    model: &str,
    value: Option<i64>,
) {
    let provider_entry = settings
        .ai
        .user_context_windows
        .entry(provider_id.to_string())
        .or_insert_with(|| serde_json::json!({}));
    let Some(provider_windows) = provider_entry.as_object_mut() else {
        return;
    };
    match value.filter(|value| (1024..=10_485_760).contains(value)) {
        Some(value) => {
            provider_windows.insert(model.to_string(), serde_json::json!(value));
        }
        None => {
            provider_windows.remove(model);
        }
    }
    if provider_windows.is_empty() {
        settings.ai.user_context_windows.remove(provider_id);
    }
}

pub fn set_ai_model_max_response_tokens(
    settings: &mut PersistedSettings,
    provider_id: &str,
    model: &str,
    value: Option<i64>,
) {
    let provider_entry = settings
        .ai
        .model_max_response_tokens
        .entry(provider_id.to_string())
        .or_insert_with(|| serde_json::json!({}));
    let Some(model_tokens) = provider_entry.as_object_mut() else {
        return;
    };
    match value.filter(|value| (256..=65_536).contains(value)) {
        Some(value) => {
            model_tokens.insert(model.to_string(), serde_json::json!(value));
        }
        None => {
            model_tokens.remove(model);
        }
    }
    if model_tokens.is_empty() {
        settings.ai.model_max_response_tokens.remove(provider_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizing_legacy_execution_profile_without_id_converges() {
        let mut settings = PersistedSettings::default();
        settings.ai.execution_profiles = serde_json::json!({
            "profiles": [{
                "name": "Migrated",
                "providerId": "custom",
                "model": "model"
            }]
        });

        assert!(ai_execution_profiles_need_normalization(&settings));
        ai_normalize_execution_profiles(&mut settings);

        assert!(!ai_execution_profiles_need_normalization(&settings));
        assert_eq!(
            settings
                .ai
                .execution_profiles
                .get("defaultProfileId")
                .and_then(serde_json::Value::as_str),
            Some("default")
        );
        assert_eq!(
            settings
                .ai
                .execution_profiles
                .get("profiles")
                .and_then(serde_json::Value::as_array)
                .and_then(|profiles| profiles.first())
                .and_then(|profile| profile.get("id"))
                .and_then(serde_json::Value::as_str),
            Some("default")
        );
    }

    #[test]
    fn ai_mcp_draft_validation_rejects_duplicates_and_invalid_names() {
        let mut settings = PersistedSettings::default();
        settings.ai.mcp_servers.push(serde_json::json!({
            "id": "existing",
            "name": "existing",
            "transport": "stdio",
            "command": "node",
            "enabled": true
        }));

        let mut draft = AiMcpServerDraft {
            name: "new-server".to_string(),
            ..AiMcpServerDraft::default()
        };
        assert!(ai_mcp_draft_valid(&draft, &settings));

        draft.name = "existing".to_string();
        assert!(!ai_mcp_draft_valid(&draft, &settings));

        draft.name = "not allowed".to_string();
        assert!(!ai_mcp_draft_valid(&draft, &settings));
    }

    #[test]
    fn ai_mcp_record_cleaning_and_arg_split_are_model_owned() {
        let record = ai_mcp_clean_record(&[
            ("TOKEN".to_string(), "abc".to_string()),
            (" ".to_string(), "ignored".to_string()),
        ])
        .unwrap();
        assert_eq!(
            record.get("TOKEN").and_then(serde_json::Value::as_str),
            Some("abc")
        );
        assert_eq!(
            ai_mcp_split_args("node server.js --stdio"),
            vec!["node", "server.js", "--stdio"]
        );
    }

    #[test]
    fn ai_mcp_draft_input_adapter_trims_identity_fields_only() {
        let mut draft = AiMcpServerDraft {
            env: vec![(String::new(), String::new())],
            ..AiMcpServerDraft::default()
        };

        assert!(apply_ai_mcp_draft_input(
            Some(&mut draft),
            SettingsInput::AiMcpName,
            " demo "
        ));
        assert!(apply_ai_mcp_draft_input(
            Some(&mut draft),
            SettingsInput::AiMcpEnvValue(0),
            " value "
        ));

        assert_eq!(
            ai_mcp_draft_input_value(Some(&draft), SettingsInput::AiMcpName).as_deref(),
            Some("demo")
        );
        assert_eq!(
            ai_mcp_draft_input_value(Some(&draft), SettingsInput::AiMcpEnvValue(0)).as_deref(),
            Some(" value ")
        );
    }

    #[test]
    fn ai_tool_policy_groups_own_policy_view_model() {
        let mut settings = PersistedSettings::default();
        settings
            .ai
            .tool_use
            .auto_approve_tools
            .insert("run_command".to_string(), serde_json::json!(true));

        let groups = ai_tool_policy_groups(&settings);

        assert!(groups.iter().any(|group| {
            group
                .items
                .iter()
                .any(|item| item.key == Some("run_command") && item.checked)
        }));
        assert_eq!(
            ai_tool_auto_approved_count(&settings),
            settings
                .ai
                .tool_use
                .auto_approve_tools
                .values()
                .filter(|value| value.as_bool() == Some(true))
                .count()
        );
    }

    #[test]
    fn ai_model_panels_own_reasoning_and_context_view_models() {
        let mut settings = PersistedSettings::default();
        settings
            .ai
            .reasoning_model_overrides
            .insert("openai".to_string(), serde_json::json!({ "gpt-5": "high" }));
        settings
            .ai
            .user_context_windows
            .insert("openai".to_string(), serde_json::json!({ "gpt-5": 128000 }));
        let providers = vec![AiProviderView {
            id: "openai".to_string(),
            provider_type: "openai".to_string(),
            name: "OpenAI".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            default_model: "gpt-5".to_string(),
            models: vec!["gpt-5".to_string(), "gpt-5-mini".to_string()],
            enabled: true,
            custom: false,
        }];

        let reasoning_panels = ai_model_reasoning_panels(&settings, &providers);
        let context_panels = ai_model_context_window_panels(&settings, &providers);

        assert_eq!(reasoning_panels[0].override_count, 1);
        assert_eq!(context_panels[0].override_count, 1);
        assert_eq!(
            ai_model_reasoning_row(&settings, "openai", "gpt-5").label_key,
            "settings_view.ai.reasoning_high"
        );
        assert_eq!(
            ai_model_reasoning_row(&settings, "openai", "gpt-5-mini").label_key,
            "settings_view.ai.reasoning_inherit_provider"
        );
        assert_eq!(
            ai_model_context_window_row(&settings, "openai", "gpt-5").source,
            ContextWindowSource::User
        );
    }

    #[test]
    fn ai_signatures_ignore_mcp_auth_token() {
        let config = oxideterm_ai::McpServerConfig {
            id: "demo".to_string(),
            name: "demo".to_string(),
            transport: McpTransport::Stdio,
            url: None,
            command: Some("node".to_string()),
            args: Vec::new(),
            env: std::collections::HashMap::new(),
            auth_header_name: None,
            auth_header_mode: None,
            headers: std::collections::HashMap::new(),
            enabled: true,
            retry_on_disconnect: false,
            auth_token: Some("secret-1".to_string()),
        };
        let mut changed_secret = config.clone();
        changed_secret.auth_token = Some("secret-2".to_string());

        assert_eq!(
            ai_mcp_server_signature(&config, None),
            ai_mcp_server_signature(&changed_secret, None)
        );
    }
}

pub const AI_MODEL_REFRESH_MISSING_API_KEY: &str = "__missing_api_key__";

#[derive(Clone, Debug)]
pub struct AiMcpServerDraft {
    pub name: String,
    pub transport: McpTransport,
    pub command: String,
    pub args: String,
    pub env: Vec<(String, String)>,
    pub url: String,
    pub auth_header_name: String,
    pub auth_header_mode: McpAuthHeaderMode,
    pub auth_token: String,
    pub headers: Vec<(String, String)>,
    pub retry_on_disconnect: bool,
    pub show_auth_token: bool,
}

impl Default for AiMcpServerDraft {
    fn default() -> Self {
        Self {
            name: String::new(),
            transport: McpTransport::Stdio,
            command: String::new(),
            args: String::new(),
            env: Vec::new(),
            url: String::new(),
            auth_header_name: "Authorization".to_string(),
            auth_header_mode: McpAuthHeaderMode::Bearer,
            auth_token: String::new(),
            headers: Vec::new(),
            retry_on_disconnect: false,
            show_auth_token: false,
        }
    }
}

pub fn ai_mcp_server_signature(
    config: &oxideterm_ai::McpServerConfig,
    snapshot: Option<&oxideterm_ai::McpServerStateSnapshot>,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    // Do not hash auth_token. The visible card is driven by public config,
    // status, endpoint, error text, and tool names.
    config.id.hash(&mut hasher);
    config.name.hash(&mut hasher);
    format!("{:?}", config.transport).hash(&mut hasher);
    config.url.hash(&mut hasher);
    config.command.hash(&mut hasher);
    config.args.hash(&mut hasher);
    config.env.len().hash(&mut hasher);
    config.auth_header_name.hash(&mut hasher);
    config
        .auth_header_mode
        .map(|mode| format!("{mode:?}"))
        .hash(&mut hasher);
    config.headers.len().hash(&mut hasher);
    config.enabled.hash(&mut hasher);
    config.retry_on_disconnect.hash(&mut hasher);
    if let Some(snapshot) = snapshot {
        snapshot.status.hash(&mut hasher);
        snapshot.endpoint_url.hash(&mut hasher);
        snapshot.error.hash(&mut hasher);
        snapshot
            .tools
            .iter()
            .for_each(|tool| tool.name.hash(&mut hasher));
    }
    hasher.finish()
}

pub fn ai_mcp_configs(settings: &PersistedSettings) -> Vec<oxideterm_ai::McpServerConfig> {
    settings
        .ai
        .mcp_servers
        .iter()
        .filter_map(|value| serde_json::from_value(value.clone()).ok())
        .collect()
}

pub fn ai_mcp_draft_valid(draft: &AiMcpServerDraft, settings: &PersistedSettings) -> bool {
    let name = draft.name.trim();
    !name.is_empty()
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
        && !ai_mcp_configs(settings)
            .iter()
            .any(|server| server.name == name)
}

pub fn ai_mcp_draft_input_value(
    draft: Option<&AiMcpServerDraft>,
    input: SettingsInput,
) -> Option<String> {
    let draft = draft?;
    match input {
        SettingsInput::AiMcpName => Some(draft.name.clone()),
        SettingsInput::AiMcpCommand => Some(draft.command.clone()),
        SettingsInput::AiMcpArgs => Some(draft.args.clone()),
        SettingsInput::AiMcpUrl => Some(draft.url.clone()),
        SettingsInput::AiMcpAuthHeaderName => Some(draft.auth_header_name.clone()),
        SettingsInput::AiMcpAuthToken => Some(draft.auth_token.clone()),
        SettingsInput::AiMcpEnvKey(index) => draft.env.get(index).map(|(key, _)| key.clone()),
        SettingsInput::AiMcpEnvValue(index) => draft.env.get(index).map(|(_, value)| value.clone()),
        SettingsInput::AiMcpHeaderKey(index) => {
            draft.headers.get(index).map(|(key, _)| key.clone())
        }
        SettingsInput::AiMcpHeaderValue(index) => {
            draft.headers.get(index).map(|(_, value)| value.clone())
        }
        _ => None,
    }
}

pub fn apply_ai_mcp_draft_input(
    draft: Option<&mut AiMcpServerDraft>,
    input: SettingsInput,
    value: &str,
) -> bool {
    let Some(draft) = draft else {
        return false;
    };
    match input {
        SettingsInput::AiMcpName => draft.name = value.trim().to_string(),
        SettingsInput::AiMcpCommand => draft.command = value.trim().to_string(),
        SettingsInput::AiMcpArgs => draft.args = value.to_string(),
        SettingsInput::AiMcpUrl => draft.url = value.trim().to_string(),
        SettingsInput::AiMcpAuthHeaderName => draft.auth_header_name = value.trim().to_string(),
        SettingsInput::AiMcpAuthToken => {
            // Auth tokens are draft-only secret input values; callers own
            // zeroizing their transient input buffer when focus leaves.
            draft.auth_token = value.to_string();
        }
        SettingsInput::AiMcpEnvKey(index) => {
            let Some((key, _)) = draft.env.get_mut(index) else {
                return false;
            };
            *key = value.trim().to_string();
        }
        SettingsInput::AiMcpEnvValue(index) => {
            let Some((_, env_value)) = draft.env.get_mut(index) else {
                return false;
            };
            *env_value = value.to_string();
        }
        SettingsInput::AiMcpHeaderKey(index) => {
            let Some((key, _)) = draft.headers.get_mut(index) else {
                return false;
            };
            *key = value.trim().to_string();
        }
        SettingsInput::AiMcpHeaderValue(index) => {
            let Some((_, header_value)) = draft.headers.get_mut(index) else {
                return false;
            };
            *header_value = value.to_string();
        }
        _ => return false,
    }
    true
}

pub fn ai_mcp_transport_label(transport: McpTransport) -> String {
    match transport {
        McpTransport::Stdio => "stdio",
        McpTransport::StreamableHttp | McpTransport::Sse => "Streamable HTTP",
        McpTransport::LegacySse => "Legacy SSE",
    }
    .to_string()
}

pub fn ai_mcp_transport_value(transport: McpTransport) -> &'static str {
    match transport {
        McpTransport::Stdio => "stdio",
        McpTransport::StreamableHttp | McpTransport::Sse => "streamable-http",
        McpTransport::LegacySse => "legacy-sse",
    }
}

pub fn ai_mcp_auth_mode_value(mode: McpAuthHeaderMode) -> &'static str {
    match mode {
        McpAuthHeaderMode::Bearer => "bearer",
        McpAuthHeaderMode::Raw => "raw",
        McpAuthHeaderMode::None => "none",
    }
}

pub fn ai_mcp_clean_record(entries: &[(String, String)]) -> Option<serde_json::Value> {
    let mut map = serde_json::Map::new();
    for (key, value) in entries {
        let key = key.trim();
        if !key.is_empty() {
            map.insert(key.to_string(), serde_json::json!(value));
        }
    }
    (!map.is_empty()).then(|| serde_json::Value::Object(map))
}

pub fn ai_mcp_split_args(args: &str) -> Vec<String> {
    args.split_whitespace().map(str::to_string).collect()
}

pub struct AiProviderKeyStatusDelivery {
    pub provider_id: String,
    pub has_key: bool,
}

pub struct AiModelRefreshDelivery {
    pub index: usize,
    pub provider_id: String,
    pub generation: u64,
    pub result: Result<ProviderModelRefresh, String>,
}
