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

pub fn ai_scoped_memory_context(
    memory: &oxideterm_settings::AiMemorySettings,
    maximum_chars: usize,
    user_id: Option<&str>,
    project_id: Option<&str>,
    host_id: Option<&str>,
) -> (Option<String>, Vec<String>) {
    if !memory.enabled {
        return (None, Vec::new());
    }
    if memory.entries.is_empty() {
        let legacy = crate::sanitize_for_ai(memory.content.trim());
        return ((!legacy.is_empty()).then_some(legacy), Vec::new());
    }

    let now_ms = ai_memory_now_ms();
    let workspace_id = oxideterm_settings::AI_APPLICATION_WORKSPACE_MEMORY_SCOPE_ID;
    let mut entries = memory
        .entries
        .iter()
        .filter(|entry| !entry.is_expired(now_ms))
        .filter(|entry| entry.applies_to(user_id, Some(workspace_id), project_id, host_id))
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| {
        let priority = match entry.scope_kind {
            oxideterm_settings::AiMemoryScopeKind::Host => 0,
            oxideterm_settings::AiMemoryScopeKind::Project => 1,
            oxideterm_settings::AiMemoryScopeKind::Workspace => 2,
            oxideterm_settings::AiMemoryScopeKind::User => 3,
        };
        (priority, std::cmp::Reverse(entry.updated_at_ms))
    });

    let mut seen = std::collections::HashSet::new();
    let mut selected_ids = Vec::new();
    let mut selected = String::new();
    for entry in entries
        .into_iter()
        .filter(|entry| seen.insert(ai_normalized_memory_content(&entry.content)))
    {
        let line = format!(
            "- [{}:{}] {}",
            ai_memory_scope_label(entry.scope_kind),
            entry.scope_id.as_deref().unwrap_or("current"),
            crate::sanitize_for_ai(&entry.content)
        );
        let separator_chars = usize::from(!selected.is_empty());
        let used_chars = selected.chars().count();
        let remaining_chars = maximum_chars.saturating_sub(used_chars + separator_chars);
        if remaining_chars == 0 {
            break;
        }
        if !selected.is_empty() {
            selected.push('\n');
        }
        selected_ids.push(entry.id.clone());
        if line.chars().count() <= remaining_chars {
            selected.push_str(&line);
            continue;
        }
        // Record usage only for entries that actually cross the model boundary.
        selected.extend(line.chars().take(remaining_chars));
        break;
    }
    ((!selected.is_empty()).then_some(selected), selected_ids)
}

pub fn ai_list_memory_entries(
    entries: &[oxideterm_settings::AiMemoryEntry],
    arguments: &serde_json::Value,
) -> serde_json::Value {
    let now_ms = ai_memory_now_ms();
    let scope_kind = arguments
        .get("scope_kind")
        .and_then(serde_json::Value::as_str);
    let scope_id = arguments
        .get("scope_id")
        .and_then(serde_json::Value::as_str);
    let memory_kind = arguments
        .get("memory_kind")
        .and_then(serde_json::Value::as_str);
    let include_expired = arguments
        .get("include_expired")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let entries = entries
        .iter()
        .filter(|entry| include_expired || !entry.is_expired(now_ms))
        .filter(|entry| {
            scope_kind.is_none_or(|kind| ai_memory_scope_label(entry.scope_kind) == kind)
        })
        .filter(|entry| scope_id.is_none_or(|scope_id| entry.scope_id.as_deref() == Some(scope_id)))
        .filter(|entry| {
            memory_kind.is_none_or(|kind| ai_memory_kind_label(entry.memory_kind) == kind)
        })
        .map(ai_memory_entry_json)
        .collect::<Vec<_>>();
    serde_json::json!({ "entries": entries })
}

pub fn ai_manage_memory_entry(
    entries: &mut Vec<oxideterm_settings::AiMemoryEntry>,
    arguments: &serde_json::Value,
    resolve_scope: impl Fn(oxideterm_settings::AiMemoryScopeKind) -> Result<Option<String>, String>,
) -> Result<serde_json::Value, String> {
    let resolve_scope = |kind| {
        if let Some(id) = arguments
            .get("scope_id")
            .and_then(serde_json::Value::as_str)
        {
            return Ok(Some(id.to_owned()));
        }
        resolve_scope(kind)
    };
    let action = arguments
        .get("action")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "A memory action is required.".to_string())?;
    let now_ms = ai_memory_now_ms();
    let result = match action {
        "create" => {
            let content = ai_memory_entry_content(arguments)?;
            let scope_kind = ai_memory_scope_from_arguments(arguments)?;
            let scope_id = resolve_scope(scope_kind)?;
            let memory_kind = ai_memory_kind_from_arguments(arguments)?;
            let expires_at_ms = ai_memory_expiry(arguments, memory_kind, now_ms)?;
            let normalized = ai_normalized_memory_content(&content);
            if let Some(existing) = entries.iter_mut().find(|entry| {
                entry.scope_kind == scope_kind
                    && entry.scope_id == scope_id
                    && ai_normalized_memory_content(&entry.content) == normalized
            }) {
                existing.last_used_at_ms = Some(now_ms);
                existing.use_count = existing.use_count.saturating_add(1);
                existing.updated_at_ms = now_ms;
                existing.revision = existing.revision.saturating_add(1);
                ai_memory_entry_json(existing)
            } else {
                let entry = oxideterm_settings::AiMemoryEntry {
                    id: uuid::Uuid::new_v4().to_string(),
                    content,
                    scope_kind,
                    scope_id,
                    memory_kind,
                    source: oxideterm_settings::AiMemorySource::Assistant,
                    created_at_ms: now_ms,
                    updated_at_ms: now_ms,
                    last_used_at_ms: None,
                    use_count: 0,
                    expires_at_ms,
                    revision: 1,
                };
                let result = ai_memory_entry_json(&entry);
                entries.push(entry);
                result
            }
        }
        "update" => {
            let (entry_id, normalized_content, scope_kind, scope_id) = {
                let entry = ai_memory_entry_mut(arguments, entries)?;
                if let Some(content) = arguments.get("content").and_then(serde_json::Value::as_str)
                {
                    let content = content.trim();
                    if !crate::preference_is_safe_to_persist(content) {
                        return Err(
                            "Memory cannot store credentials or one-time task instructions."
                                .to_string(),
                        );
                    }
                    entry.content = content.to_string();
                }
                if arguments.get("scope_kind").is_some() {
                    entry.scope_kind = ai_memory_scope_from_arguments(arguments)?;
                    entry.scope_id = resolve_scope(entry.scope_kind)?;
                }
                if arguments.get("memory_kind").is_some() {
                    entry.memory_kind = ai_memory_kind_from_arguments(arguments)?;
                }
                if arguments.get("memory_kind").is_some()
                    || arguments.get("expires_at_ms").is_some()
                    || entry.memory_kind == oxideterm_settings::AiMemoryKind::Temporary
                {
                    entry.expires_at_ms = ai_memory_expiry(arguments, entry.memory_kind, now_ms)?;
                }
                entry.updated_at_ms = now_ms;
                entry.revision = entry.revision.saturating_add(1);
                (
                    entry.id.clone(),
                    ai_normalized_memory_content(&entry.content),
                    entry.scope_kind,
                    entry.scope_id.clone(),
                )
            };
            if entries.iter().any(|entry| {
                entry.id != entry_id
                    && entry.scope_kind == scope_kind
                    && entry.scope_id == scope_id
                    && ai_normalized_memory_content(&entry.content) == normalized_content
            }) {
                return Err("An equivalent memory entry already exists in this scope.".to_string());
            }
            let entry = entries
                .iter()
                .find(|entry| entry.id == entry_id)
                .ok_or_else(|| "The memory entry no longer exists.".to_string())?;
            ai_memory_entry_json(entry)
        }
        "touch" => {
            let entry = ai_memory_entry_mut(arguments, entries)?;
            entry.last_used_at_ms = Some(now_ms);
            entry.use_count = entry.use_count.saturating_add(1);
            entry.updated_at_ms = now_ms;
            entry.revision = entry.revision.saturating_add(1);
            ai_memory_entry_json(entry)
        }
        "delete" => {
            let id = ai_memory_id(arguments)?;
            let expected_revision = ai_memory_expected_revision(arguments)?;
            let index = entries
                .iter()
                .position(|entry| entry.id == id)
                .ok_or_else(|| "The memory entry no longer exists.".to_string())?;
            if entries[index].revision != expected_revision {
                return Err("The memory entry changed; refresh it before deleting.".to_string());
            }
            entries.remove(index);
            serde_json::json!({ "id": id, "deleted": true })
        }
        _ => return Err("Unsupported memory action.".to_string()),
    };
    Ok(serde_json::json!({ "accepted": true, "action": action, "entry": result }))
}

pub fn ai_remember_preference_payload<'a>(
    snapshot: &super::AiOrchestratorRuntimeSnapshot,
    args: &'a serde_json::Value,
) -> Result<&'a str, super::AiActionResultLite> {
    let Some(preference) = args
        .get("preference")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Err(snapshot.fail(
            "Preference is required.",
            "missing_preference",
            "remember_preference requires preference text.",
            "write",
        ));
    };
    if !crate::preference_is_safe_to_persist(preference) {
        return Err(snapshot.fail(
            "Preference was not saved.",
            "memory_content_rejected",
            "Long-term memory cannot store credentials or one-time task instructions.",
            "write",
        ));
    }

    Ok(preference)
}

pub fn save_ai_user_preference(
    memory: &mut oxideterm_settings::AiMemorySettings,
    preference: String,
) {
    let now_ms = ai_memory_now_ms();
    let normalized = ai_normalized_memory_content(&preference);
    if let Some(existing) = memory.entries.iter_mut().find(|entry| {
        entry.scope_kind == oxideterm_settings::AiMemoryScopeKind::User
            && entry.scope_id.is_none()
            && ai_normalized_memory_content(&entry.content) == normalized
    }) {
        existing.last_used_at_ms = Some(now_ms);
        existing.use_count = existing.use_count.saturating_add(1);
        existing.updated_at_ms = now_ms;
        existing.revision = existing.revision.saturating_add(1);
        return;
    }
    memory.entries.push(oxideterm_settings::AiMemoryEntry {
        id: uuid::Uuid::new_v4().to_string(),
        content: preference,
        scope_kind: oxideterm_settings::AiMemoryScopeKind::User,
        scope_id: None,
        memory_kind: oxideterm_settings::AiMemoryKind::LongTerm,
        source: oxideterm_settings::AiMemorySource::Assistant,
        created_at_ms: now_ms,
        updated_at_ms: now_ms,
        last_used_at_ms: None,
        use_count: 0,
        expires_at_ms: None,
        revision: 1,
    });
}

pub fn ai_preference_saved(
    snapshot: &super::AiOrchestratorRuntimeSnapshot,
) -> super::AiActionResultLite {
    snapshot.ok(
        "Preference remembered.",
        "The preference was saved as a user-scoped memory entry.",
        serde_json::json!({ "scopeKind": "user", "memoryKind": "long_term" }),
        "write",
    )
}

pub fn record_memory_usage(
    memory: &mut oxideterm_settings::AiMemorySettings,
    entry_ids: &[String],
) {
    let now_ms = ai_memory_now_ms();
    let ids: std::collections::HashSet<_> = entry_ids.iter().collect();
    for entry in &mut memory.entries {
        if ids.contains(&entry.id) {
            entry.last_used_at_ms = Some(now_ms);
            entry.use_count = entry.use_count.saturating_add(1);
        }
    }
}
