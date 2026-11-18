pub fn ai_memory_now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

pub fn ai_memory_scope_label(scope: oxideterm_settings::AiMemoryScopeKind) -> &'static str {
    match scope {
        oxideterm_settings::AiMemoryScopeKind::User => "user",
        oxideterm_settings::AiMemoryScopeKind::Workspace => "workspace",
        oxideterm_settings::AiMemoryScopeKind::Project => "project",
        oxideterm_settings::AiMemoryScopeKind::Host => "host",
    }
}

pub fn ai_memory_kind_label(kind: oxideterm_settings::AiMemoryKind) -> &'static str {
    match kind {
        oxideterm_settings::AiMemoryKind::LongTerm => "long_term",
        oxideterm_settings::AiMemoryKind::Temporary => "temporary",
    }
}

pub fn ai_memory_entry_json(entry: &oxideterm_settings::AiMemoryEntry) -> serde_json::Value {
    serde_json::json!({
        "id": entry.id,
        "content": crate::sanitize_for_ai(&entry.content),
        "scopeKind": ai_memory_scope_label(entry.scope_kind),
        "scopeId": entry.scope_id,
        "memoryKind": ai_memory_kind_label(entry.memory_kind),
        "source": format!("{:?}", entry.source).to_lowercase(),
        "createdAtMs": entry.created_at_ms,
        "updatedAtMs": entry.updated_at_ms,
        "lastUsedAtMs": entry.last_used_at_ms,
        "useCount": entry.use_count,
        "expiresAtMs": entry.expires_at_ms,
        "revision": entry.revision,
    })
}

pub fn ai_memory_entry_content(arguments: &serde_json::Value) -> Result<String, String> {
    let content = arguments
        .get("content")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|content| !content.is_empty())
        .ok_or_else(|| "Memory content is required.".to_string())?;
    if !crate::preference_is_safe_to_persist(content) {
        return Err("Memory cannot store credentials or one-time task instructions.".to_string());
    }
    Ok(content.to_string())
}

pub fn ai_memory_scope_from_arguments(
    arguments: &serde_json::Value,
) -> Result<oxideterm_settings::AiMemoryScopeKind, String> {
    match arguments
        .get("scope_kind")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("user")
    {
        "user" => Ok(oxideterm_settings::AiMemoryScopeKind::User),
        "workspace" => Ok(oxideterm_settings::AiMemoryScopeKind::Workspace),
        "project" => Ok(oxideterm_settings::AiMemoryScopeKind::Project),
        "host" => Ok(oxideterm_settings::AiMemoryScopeKind::Host),
        _ => Err("Unsupported memory scope.".to_string()),
    }
}

pub fn ai_memory_kind_from_arguments(
    arguments: &serde_json::Value,
) -> Result<oxideterm_settings::AiMemoryKind, String> {
    match arguments
        .get("memory_kind")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("long_term")
    {
        "long_term" => Ok(oxideterm_settings::AiMemoryKind::LongTerm),
        "temporary" => Ok(oxideterm_settings::AiMemoryKind::Temporary),
        _ => Err("Unsupported memory kind.".to_string()),
    }
}

pub fn ai_memory_expiry(
    arguments: &serde_json::Value,
    memory_kind: oxideterm_settings::AiMemoryKind,
    now_ms: i64,
) -> Result<Option<i64>, String> {
    let expires_at_ms = arguments
        .get("expires_at_ms")
        .and_then(serde_json::Value::as_i64);
    if memory_kind == oxideterm_settings::AiMemoryKind::Temporary {
        let expires_at_ms = expires_at_ms
            .filter(|expires_at_ms| *expires_at_ms > now_ms)
            .ok_or_else(|| "Temporary memory requires a future expiry time.".to_string())?;
        Ok(Some(expires_at_ms))
    } else {
        Ok(expires_at_ms.filter(|expires_at_ms| *expires_at_ms > now_ms))
    }
}

pub fn ai_normalized_memory_content(content: &str) -> String {
    content
        .split_whitespace()
        .flat_map(str::chars)
        .flat_map(char::to_lowercase)
        .collect()
}

pub fn ai_memory_id(arguments: &serde_json::Value) -> Result<&str, String> {
    arguments
        .get("id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "A memory entry identifier is required.".to_string())
}

pub fn ai_memory_expected_revision(arguments: &serde_json::Value) -> Result<u64, String> {
    arguments
        .get("expected_revision")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "The current memory revision is required.".to_string())
}

pub fn ai_memory_entry_mut<'a>(
    arguments: &serde_json::Value,
    entries: &'a mut [oxideterm_settings::AiMemoryEntry],
) -> Result<&'a mut oxideterm_settings::AiMemoryEntry, String> {
    let id = ai_memory_id(arguments)?;
    let expected_revision = ai_memory_expected_revision(arguments)?;
    let entry = entries
        .iter_mut()
        .find(|entry| entry.id == id)
        .ok_or_else(|| "The memory entry no longer exists.".to_string())?;
    if entry.revision != expected_revision {
        return Err("The memory entry changed; refresh it before updating.".to_string());
    }
    Ok(entry)
}

pub fn ai_memory_settings_json(
    enabled: bool,
    content: &str,
    entries: &[oxideterm_settings::AiMemoryEntry],
) -> serde_json::Value {
    // Keep the legacy content field while exposing the itemized model.
    serde_json::json!({
        "enabled": enabled,
        "content": content,
        "entries": entries.iter().map(ai_memory_entry_json).collect::<Vec<_>>(),
    })
}

pub fn ai_memory_content(memory: &serde_json::Value) -> &str {
    memory
        .get("content")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
}

pub fn ai_memory_trimmed_content(memory: &serde_json::Value) -> &str {
    ai_memory_content(memory).trim()
}
