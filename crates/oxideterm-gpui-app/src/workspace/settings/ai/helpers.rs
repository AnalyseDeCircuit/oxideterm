fn ai_provider_views(settings: &PersistedSettings) -> Vec<AiProviderView> {
    ai_provider_views_from_values(&settings.ai.providers)
}

fn ai_update_provider(
    settings: &mut PersistedSettings,
    index: usize,
    update: impl FnOnce(&mut serde_json::Map<String, serde_json::Value>),
) {
    ai_update_provider_values(&mut settings.ai.providers, index, update);
}

fn toggle_string_set(set: &mut HashSet<String>, value: &str) {
    if !set.remove(value) {
        set.insert(value.to_string());
    }
}

fn current_time_millis() -> u128 {
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

fn ai_patch_execution_profile(
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

fn ai_execution_profile_id(profile: &serde_json::Value) -> Option<String> {
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

fn ai_execution_profiles_need_normalization(settings: &PersistedSettings) -> bool {
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
        !profiles
            .iter()
            .any(|profile| profile.get("id").and_then(serde_json::Value::as_str) == Some(default_id))
    })
}

fn ai_normalize_execution_profiles(settings: &mut PersistedSettings) {
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
        profiles
            .iter()
            .any(|profile| profile.get("id").and_then(serde_json::Value::as_str) == Some(default_id))
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

fn ai_default_execution_profile(settings: &PersistedSettings) -> Option<String> {
    settings
        .ai
        .execution_profiles
        .get("defaultProfileId")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

fn ai_add_execution_profile(settings: &mut PersistedSettings) {
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
        object.insert("defaultProfileId".to_string(), serde_json::json!(profile_id));
    }
}

fn ai_duplicate_execution_profile(settings: &mut PersistedSettings, index: usize) {
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
        object.insert("name".to_string(), serde_json::json!(format!("{name} Copy")));
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

fn ai_delete_execution_profile(settings: &mut PersistedSettings, index: usize) {
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

fn ai_set_default_execution_profile(settings: &mut PersistedSettings, profile_id: String) {
    if let Some(object) = settings.ai.execution_profiles.as_object_mut() {
        object.insert("defaultProfileId".to_string(), serde_json::json!(profile_id));
    }
}

fn ai_reasoning_profile_value(effort: oxideterm_settings::AiReasoningEffort) -> &'static str {
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

fn ai_reasoning_effort_from_profile_value(value: &str) -> oxideterm_settings::AiReasoningEffort {
    match value {
        "off" => oxideterm_settings::AiReasoningEffort::None,
        "low" => oxideterm_settings::AiReasoningEffort::Low,
        "medium" => oxideterm_settings::AiReasoningEffort::Medium,
        "high" => oxideterm_settings::AiReasoningEffort::High,
        "max" => oxideterm_settings::AiReasoningEffort::Xhigh,
        _ => oxideterm_settings::AiReasoningEffort::Auto,
    }
}

fn set_ai_provider_reasoning_override(
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

fn set_ai_model_reasoning_override(
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

fn set_ai_user_context_window(
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

fn set_ai_model_max_response_tokens(
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
}
