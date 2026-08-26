// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use anyhow::{Context, Result};
use serde_json::{Map, Value, json};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
    ParsedTerminalSessionLogTemplate, TerminalSessionLogTemplateError, model::*,
    parse_terminal_session_log_content_template, parse_terminal_session_log_file_name_template,
};

#[derive(Clone, Debug, PartialEq)]
pub struct SanitizedSettings {
    pub settings: PersistedSettings,
    pub migration_warnings: Vec<String>,
    pub validation_warnings: Vec<String>,
}

fn merge_json(defaults: &mut Value, incoming: &Value) {
    match (defaults, incoming) {
        (Value::Object(default_map), Value::Object(incoming_map)) => {
            for (key, value) in incoming_map {
                if let Some(target) = default_map.get_mut(key) {
                    merge_json(target, value);
                } else {
                    default_map.insert(key.clone(), value.clone());
                }
            }
        }
        (target, incoming_value) => *target = incoming_value.clone(),
    }
}

fn get_path_mut<'a>(value: &'a mut Value, path: &[&str]) -> Option<&'a mut Value> {
    let mut current = value;
    for segment in path {
        current = current.get_mut(*segment)?;
    }
    Some(current)
}

fn object_mut<'a>(value: &'a mut Value, key: &str) -> Option<&'a mut Map<String, Value>> {
    value.get_mut(key).and_then(Value::as_object_mut)
}

fn normalize_sftp_speed_limit_key(settings: &mut Value, raw: &Value) {
    let Some(sftp) = object_mut(settings, "sftp") else {
        return;
    };
    let Some(value) = sftp.remove("speedLimitKbps") else {
        return;
    };

    if raw
        .get("sftp")
        .and_then(|settings| settings.get("speedLimitKBps"))
        .is_some()
    {
        return;
    }

    // Keep the Tauri spelling canonical while still accepting older native
    // files that used serde's plain camelCase acronym handling.
    sftp.insert("speedLimitKBps".to_string(), value);
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

fn migrate_ai_memory_entries(settings: &mut Value) {
    let Some(memory) = settings
        .get_mut("ai")
        .and_then(Value::as_object_mut)
        .and_then(|ai| ai.get_mut("memory"))
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    if memory
        .get("entries")
        .and_then(Value::as_array)
        .is_some_and(|entries| !entries.is_empty())
    {
        return;
    }
    let Some(content) = memory
        .get("content")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|content| !content.is_empty())
    else {
        return;
    };
    // One deterministic migrated entry prevents duplicate imports on every load.
    memory.insert(
        "entries".to_string(),
        json!([{
            "id": "legacy-user-memory",
            "content": content,
            "scopeKind": "user",
            "memoryKind": "long_term",
            "source": "migrated",
            "createdAtMs": 0,
            "updatedAtMs": 0,
            "useCount": 0,
            "revision": 1
        }]),
    );
}

fn migrate_ai_providers(settings: &mut Value, warnings: &mut Vec<String>) {
    let Some(ai) = settings.get_mut("ai").and_then(Value::as_object_mut) else {
        return;
    };
    if ai
        .get("providers")
        .and_then(Value::as_array)
        .is_some_and(|providers| !providers.is_empty())
    {
        return;
    }

    let base_url = ai
        .get("baseUrl")
        .and_then(Value::as_str)
        .unwrap_or("https://api.openai.com/v1")
        .to_string();
    let legacy_model = ai
        .get("model")
        .and_then(Value::as_str)
        .filter(|model| !model.trim().is_empty())
        .unwrap_or("gpt-4o-mini")
        .to_string();
    let created_at = now_ms();
    let mut providers = vec![
        json!({
            "id": "builtin-openai",
            "type": "openai",
            "name": "OpenAI",
            "baseUrl": "https://api.openai.com/v1",
            "models": ["gpt-4o-mini"],
            "enabled": true,
            "createdAt": created_at,
        }),
        json!({
            "id": "builtin-anthropic",
            "type": "anthropic",
            "name": "Anthropic",
            "baseUrl": "https://api.anthropic.com",
            "models": ["claude-sonnet-4-20250514"],
            "enabled": true,
            "createdAt": created_at,
        }),
        json!({
            "id": "builtin-deepseek",
            "type": "deepseek",
            "name": "DeepSeek",
            "baseUrl": "https://api.deepseek.com",
            "models": ["deepseek-v4-flash", "deepseek-v4-pro", "deepseek-chat", "deepseek-reasoner"],
            "enabled": true,
            "createdAt": created_at,
        }),
        json!({
            "id": "builtin-gemini",
            "type": "gemini",
            "name": "Google Gemini",
            "baseUrl": "https://generativelanguage.googleapis.com/v1beta",
            "models": ["gemini-2.0-flash"],
            "enabled": true,
            "createdAt": created_at,
        }),
        json!({
            "id": "builtin-ollama",
            "type": "ollama",
            "name": "Ollama (Local)",
            "baseUrl": "http://localhost:11434",
            "models": [],
            "enabled": false,
            "createdAt": created_at,
        }),
    ];

    let default_openai_url = "https://api.openai.com/v1";
    let active_provider_id = if !base_url.is_empty() && base_url != default_openai_url {
        providers.insert(
            0,
            json!({
                "id": format!("custom-migrated-{created_at}"),
                "type": "openai_compatible",
                "name": "Custom (Migrated)",
                "baseUrl": base_url,
                "models": [legacy_model],
                "enabled": true,
                "createdAt": created_at,
            }),
        );
        providers
            .first()
            .and_then(|provider| provider.get("id"))
            .cloned()
            .unwrap_or_else(|| json!("builtin-openai"))
    } else {
        json!("builtin-openai")
    };

    ai.insert("providers".to_string(), Value::Array(providers));
    ai.insert("activeProviderId".to_string(), active_provider_id);
    ai.insert("activeModel".to_string(), Value::Null);
    warnings.push("Migrated AI settings to multi-provider format".to_string());
}

fn remove_ai_provider_default_models(settings: &mut Value) {
    let Some(providers) = settings
        .get_mut("ai")
        .and_then(|ai| ai.get_mut("providers"))
        .and_then(Value::as_array_mut)
    else {
        return;
    };
    for provider in providers {
        if let Some(provider) = provider.as_object_mut() {
            provider.remove("defaultModel");
        }
    }
}

fn normalize_ai_tool_auto_approve_keys(settings: &mut Value, raw: &Value) {
    let legacy_write_enabled = raw
        .get("ai")
        .and_then(|ai| ai.get("toolUse"))
        .and_then(|tool_use| tool_use.get("autoApproveTools"))
        .and_then(|auto_approve| auto_approve.get("write_resource"))
        .and_then(Value::as_bool)
        == Some(true);
    if !legacy_write_enabled {
        return;
    }

    let raw_auto_approve = raw
        .get("ai")
        .and_then(|ai| ai.get("toolUse"))
        .and_then(|tool_use| tool_use.get("autoApproveTools"))
        .and_then(Value::as_object);
    let Some(auto_approve) = settings
        .get_mut("ai")
        .and_then(|ai| ai.get_mut("toolUse"))
        .and_then(|tool_use| tool_use.get_mut("autoApproveTools"))
        .and_then(Value::as_object_mut)
    else {
        return;
    };

    // Tauri maps the old broad write_resource approval to both granular write
    // scopes unless the saved config already chose each granular value.
    if !raw_auto_approve.is_some_and(|saved| saved.contains_key("write_resource:settings")) {
        auto_approve.insert("write_resource:settings".to_string(), json!(true));
    }
    if !raw_auto_approve.is_some_and(|saved| saved.contains_key("write_resource:file")) {
        auto_approve.insert("write_resource:file".to_string(), json!(true));
    }
}

fn migrate_acp_agent_presets(settings: &mut Value, warnings: &mut Vec<String>) {
    let Some(agents) = settings
        .get_mut("ai")
        .and_then(|ai| ai.get_mut("acpAgents"))
        .and_then(Value::as_array_mut)
    else {
        return;
    };

    let mut migrated = 0;
    for agent in agents {
        let Some(agent) = agent.as_object_mut() else {
            continue;
        };
        let command = agent
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let args = agent
            .get("args")
            .and_then(Value::as_array)
            .map(|args| args.iter().filter_map(Value::as_str).collect::<Vec<&str>>())
            .unwrap_or_default();

        let Some((new_command, new_args)) = acp_agent_preset_migration(command, &args) else {
            continue;
        };
        agent.insert("command".to_string(), json!(new_command));
        agent.insert("args".to_string(), json!(new_args));
        migrated += 1;
    }

    if migrated > 0 {
        warnings.push(format!(
            "Migrated {migrated} ACP agent preset(s) to current launch commands"
        ));
    }
}

fn acp_agent_preset_migration(
    command: &str,
    args: &[&str],
) -> Option<(&'static str, Vec<&'static str>)> {
    match (command, args) {
        ("npx", ["-y", "@agentclientprotocol/claude-agent-acp"]) => {
            Some(("oxideterm-native", vec!["--acp-adapter", "claude-code"]))
        }
        ("codex-acp", []) => Some(("oxideterm-native", vec!["--acp-adapter", "codex"])),
        // Copilot already exposes native ACP over stdio; undo older OxideTerm
        // migrations that wrapped it as a text CLI adapter.
        ("oxideterm-native", ["--acp-adapter", "github-copilot"])
        | ("oxideterm", ["--acp-adapter", "github-copilot"]) => {
            Some(("copilot", vec!["--acp", "--stdio"]))
        }
        _ => None,
    }
}

fn migrate_ai_tool_use_settings(settings: &mut Value, raw: &Value) {
    let Some(raw_tool_use) = raw
        .get("ai")
        .and_then(|ai| ai.get("toolUse"))
        .and_then(Value::as_object)
    else {
        return;
    };
    if raw_tool_use
        .get("autoApproveTools")
        .is_some_and(Value::is_object)
    {
        // Existing settings files predate newly added application tools.
        // Merge only missing defaults so user decisions remain authoritative
        // while the policy UI and exposed tool catalog stay synchronized.
        if let Some(auto_approve) = settings
            .get_mut("ai")
            .and_then(|ai| ai.get_mut("toolUse"))
            .and_then(|tool_use| tool_use.get_mut("autoApproveTools"))
            .and_then(Value::as_object_mut)
        {
            for (name, value) in AiToolUseSettings::default().auto_approve_tools {
                auto_approve.entry(name).or_insert(value);
            }
        }
        return;
    }

    let old_read_only = raw_tool_use
        .get("autoApproveReadOnly")
        .and_then(Value::as_bool)
        != Some(false);
    let old_all = raw_tool_use.get("autoApproveAll").and_then(Value::as_bool) == Some(true);
    let default_tool_use = AiToolUseSettings::default();
    let auto_approve = default_tool_use
        .auto_approve_tools
        .into_iter()
        .map(|(name, default_value)| {
            let enabled = old_all || (old_read_only && default_value.as_bool() == Some(true));
            (name, json!(enabled))
        })
        .collect::<Map<String, Value>>();

    let Some(tool_use) = settings
        .get_mut("ai")
        .and_then(|ai| ai.get_mut("toolUse"))
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    tool_use.insert("autoApproveTools".to_string(), Value::Object(auto_approve));
    tool_use.insert("disabledTools".to_string(), json!([]));
    tool_use.insert("maxRounds".to_string(), json!(DEFAULT_AI_TOOL_MAX_ROUNDS));
    // Tauri replaces the old toolUse object, so legacy flags must not survive
    // into serde flatten extras or settings snapshots.
    tool_use.remove("autoApproveReadOnly");
    tool_use.remove("autoApproveAll");
}

fn migrate_ai_execution_profile_selection(settings: &mut Value, raw: &Value) {
    let Some(ai) = settings.get_mut("ai").and_then(Value::as_object_mut) else {
        return;
    };
    if let Some(profile) = selected_ai_execution_profile(raw) {
        if profile.get("backend").and_then(Value::as_str) == Some("acp") {
            ai.insert("activeBackend".to_string(), json!("acp"));
            ai.insert(
                "activeAcpAgentId".to_string(),
                profile.get("acpAgentId").cloned().unwrap_or(Value::Null),
            );
        } else {
            ai.insert("activeBackend".to_string(), json!("provider"));
            if let Some(provider_id) = profile.get("providerId").cloned() {
                ai.insert("activeProviderId".to_string(), provider_id);
            }
            if let Some(model) = profile.get("model").cloned() {
                ai.insert("activeModel".to_string(), model);
            }
        }
        if let Some(reasoning) = profile.get("reasoningEffort").and_then(Value::as_str) {
            ai.insert(
                "reasoningEffort".to_string(),
                json!(ai_reasoning_settings_value(reasoning)),
            );
        }
        if let Some(tool_use) = profile.get("toolUse").and_then(Value::as_object) {
            merge_ai_tool_use_settings(ai, tool_use);
        }
    }
    // Execution profiles are a removed UX layer; do not preserve them through
    // serde flatten extras after migrating the active selection.
    ai.remove("executionProfiles");
}

fn selected_ai_execution_profile(raw: &Value) -> Option<&Value> {
    let config = raw.get("ai").and_then(|ai| ai.get("executionProfiles"))?;
    let profiles = config.get("profiles").and_then(Value::as_array)?;
    if profiles.is_empty() {
        return None;
    }
    config
        .get("defaultProfileId")
        .and_then(Value::as_str)
        .and_then(|default_id| {
            profiles
                .iter()
                .find(|profile| profile.get("id").and_then(Value::as_str) == Some(default_id))
        })
        .or_else(|| profiles.first())
}

fn merge_ai_tool_use_settings(ai: &mut Map<String, Value>, profile_tool_use: &Map<String, Value>) {
    let tool_use = ai.entry("toolUse".to_string()).or_insert_with(|| {
        serde_json::to_value(AiToolUseSettings::default()).unwrap_or_else(|_| json!({}))
    });
    let Some(tool_use) = tool_use.as_object_mut() else {
        return;
    };
    for (key, value) in profile_tool_use {
        tool_use.insert(key.clone(), value.clone());
    }
}

fn normalize_ai_reasoning_effort_aliases(settings: &mut Value) {
    let Some(ai) = settings.get_mut("ai").and_then(Value::as_object_mut) else {
        return;
    };
    let Some(current) = ai.get("reasoningEffort").and_then(Value::as_str) else {
        return;
    };
    let Some(normalized) = (match current {
        "off" => Some("none"),
        _ => None,
    }) else {
        return;
    };
    ai.insert("reasoningEffort".to_string(), json!(normalized));
}

fn ai_reasoning_profile_value(value: &str) -> &'static str {
    match value {
        "none" | "off" => "none",
        "minimal" => "minimal",
        "low" => "low",
        "medium" => "medium",
        "high" => "high",
        "xhigh" => "xhigh",
        "max" => "max",
        _ => "auto",
    }
}

fn ai_reasoning_settings_value(value: &str) -> &'static str {
    match ai_reasoning_profile_value(value) {
        other => other,
    }
}

fn clamp_i64(
    value: &mut Value,
    fallback: i64,
    min: i64,
    max: i64,
    path: &str,
    warnings: &mut Vec<String>,
) {
    let Some(number) = value
        .as_i64()
        .or_else(|| value.as_f64().map(|v| v.round() as i64))
    else {
        *value = json!(fallback);
        warnings.push(format!("{} reset to default {}", path, fallback));
        return;
    };
    let clamped = number.clamp(min, max);
    if clamped != number {
        warnings.push(format!("{} clamped from {} to {}", path, number, clamped));
    }
    *value = json!(clamped);
}

fn clamp_f64(
    value: &mut Value,
    fallback: f64,
    min: f64,
    max: f64,
    path: &str,
    warnings: &mut Vec<String>,
) {
    let Some(number) = value.as_f64() else {
        *value = json!(fallback);
        warnings.push(format!("{} reset to default {}", path, fallback));
        return;
    };
    let clamped = number.clamp(min, max);
    if (clamped - number).abs() > f64::EPSILON {
        warnings.push(format!("{} clamped from {} to {}", path, number, clamped));
    }
    *value = json!(clamped);
}

fn sanitize_enum(
    root: &mut Value,
    path: &[&str],
    allowed: &[&str],
    fallback: &str,
    warnings: &mut Vec<String>,
) {
    let Some(value) = get_path_mut(root, path) else {
        return;
    };
    if value.as_str().is_some_and(|item| allowed.contains(&item)) {
        return;
    }
    *value = json!(fallback);
    warnings.push(format!("{} reset to {}", path.join("."), fallback));
}

fn clamp_backend_hot_lines(lines: i64) -> i64 {
    lines.clamp(BACKEND_HOT_BUFFER_MIN, BACKEND_HOT_BUFFER_MAX)
}

fn clamp_terminal_scrollback(lines: i64) -> i64 {
    lines.clamp(TERMINAL_SCROLLBACK_MIN, TERMINAL_SCROLLBACK_MAX)
}

fn sanitize_custom_semantic_schemes(root: &mut Value, warnings: &mut Vec<String>) {
    let Some(terminal) = root.get_mut("terminal").and_then(Value::as_object_mut) else {
        return;
    };
    let Some(schemes) = terminal
        .get_mut("customSemanticSchemes")
        .and_then(Value::as_array_mut)
    else {
        return;
    };

    let original_scheme_count = schemes.len();
    let mut valid_ids = std::collections::HashSet::new();
    let mut sanitized = Vec::new();
    for value in schemes.drain(..).take(MAX_CUSTOM_SEMANTIC_SCHEMES) {
        let Ok(document) = serde_json::from_value::<
            oxideterm_terminal_semantic::SemanticSchemeDocument,
        >(value.clone()) else {
            warnings.push("Removed malformed custom semantic scheme".to_string());
            continue;
        };
        if oxideterm_terminal_semantic::validate_scheme_document(&document).is_err()
            || !valid_ids.insert(document.id.clone())
        {
            warnings.push(format!(
                "Removed invalid or duplicate custom semantic scheme: {}",
                document.id
            ));
            continue;
        }
        sanitized.push(value);
    }
    if original_scheme_count > MAX_CUSTOM_SEMANTIC_SCHEMES {
        warnings.push(format!(
            "Custom semantic schemes limited to {MAX_CUSTOM_SEMANTIC_SCHEMES}"
        ));
    }
    *schemes = sanitized;

    let active_is_valid = terminal
        .get("semanticCustomScheme")
        .and_then(Value::as_str)
        .is_some_and(|id| valid_ids.contains(id));
    if !active_is_valid {
        terminal.insert("semanticCustomScheme".to_string(), Value::Null);
    }

    if let Some(shell_schemes) = root
        .get_mut("localTerminal")
        .and_then(Value::as_object_mut)
        .and_then(|local| local.get_mut("semanticSchemeByShell"))
        .and_then(Value::as_object_mut)
    {
        shell_schemes.retain(|shell_id, scheme_id| {
            let valid_shell_id = !shell_id.trim().is_empty() && shell_id.len() <= 128;
            let valid_scheme_id = scheme_id.as_str().is_some_and(|scheme_id| {
                matches!(scheme_id, "balanced" | "conservative") || valid_ids.contains(scheme_id)
            });
            valid_shell_id && valid_scheme_id
        });
    }
}

fn sanitize_highlight_rule_sets(root: &mut Value, warnings: &mut Vec<String>) {
    let Some(terminal) = root.get_mut("terminal").and_then(Value::as_object_mut) else {
        return;
    };
    let Some(value) = terminal.get_mut("highlightRuleSets") else {
        return;
    };
    let original_count = value.as_array().map(Vec::len).unwrap_or_default();
    *value = sanitize_highlight_rule_sets_value(value);
    let valid_ids = value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|rule_set| rule_set.get("id").and_then(Value::as_str))
        .map(str::to_string)
        .collect::<std::collections::HashSet<_>>();
    if original_count > MAX_HIGHLIGHT_RULE_SETS {
        warnings.push(format!(
            "Highlight rule sets limited to {MAX_HIGHLIGHT_RULE_SETS}"
        ));
    }
    let default_is_valid = terminal
        .get("defaultHighlightRuleSet")
        .and_then(Value::as_str)
        .is_some_and(|id| valid_ids.contains(id));
    if !default_is_valid {
        terminal.insert("defaultHighlightRuleSet".to_string(), Value::Null);
    }
}

fn derive_backend_hot_lines(scrollback: i64) -> i64 {
    clamp_backend_hot_lines(clamp_terminal_scrollback(scrollback) * 2)
}

pub fn sanitize_settings_value(raw: Value) -> Result<SanitizedSettings> {
    let saved_version = raw.get("version").and_then(Value::as_u64).unwrap_or(0);
    if saved_version > u64::from(SETTINGS_SCHEMA_VERSION) {
        anyhow::bail!(
            "settings version {saved_version} is newer than supported version {SETTINGS_SCHEMA_VERSION}"
        );
    }
    let mut migration_warnings = Vec::new();
    let mut validation_warnings = Vec::new();
    let mut settings = PersistedSettings::default().to_value();

    merge_json(&mut settings, &raw);
    if let Some(object) = settings.as_object_mut() {
        object.insert("version".to_string(), json!(SETTINGS_SCHEMA_VERSION));
    }
    normalize_sftp_speed_limit_key(&mut settings, &raw);
    migrate_ai_providers(&mut settings, &mut migration_warnings);
    remove_ai_provider_default_models(&mut settings);
    migrate_ai_tool_use_settings(&mut settings, &raw);
    normalize_ai_tool_auto_approve_keys(&mut settings, &raw);
    migrate_ai_memory_entries(&mut settings);
    migrate_acp_agent_presets(&mut settings, &mut migration_warnings);
    migrate_ai_execution_profile_selection(&mut settings, &raw);

    if saved_version < u64::from(SETTINGS_SCHEMA_VERSION)
        && let Some(old_scrollback) = raw
            .get("terminal")
            .and_then(|terminal| terminal.get("scrollback"))
            .and_then(Value::as_i64)
    {
        if let Some(value) = get_path_mut(&mut settings, &["terminal", "scrollback"]) {
            *value = json!(old_scrollback.min(DEFAULT_TERMINAL_SCROLLBACK));
        }
        if let Some(value) = get_path_mut(&mut settings, &["buffer", "maxLines"]) {
            *value = json!(derive_backend_hot_lines(old_scrollback));
        }
        migration_warnings.push(
            "Migrated legacy terminal.scrollback into terminal.scrollback + buffer.maxLines"
                .to_string(),
        );
    }

    for (path, fallback, min, max) in [
        (
            "terminal.scrollback",
            DEFAULT_TERMINAL_SCROLLBACK,
            TERMINAL_SCROLLBACK_MIN,
            TERMINAL_SCROLLBACK_MAX,
        ),
        (
            "buffer.maxLines",
            DEFAULT_BACKEND_HOT_BUFFER_LINES,
            BACKEND_HOT_BUFFER_MIN,
            BACKEND_HOT_BUFFER_MAX,
        ),
        ("terminal.fontSize", 14, 8, 32),
        ("terminal.backgroundBlur", 0, 0, 20),
        ("appearance.borderRadius", 6, 0, 16),
        ("appearance.uiFontSize", DEFAULT_UI_FONT_SIZE, 11, 20),
        ("connectionDefaults.port", 22, 1, 65_535),
        ("sidebarUI.width", 300, 200, 600),
        ("sidebarUI.aiSidebarWidth", 340, 280, 500),
        ("sftp.maxConcurrentTransfers", 3, 1, 10),
        ("sftp.directoryParallelism", 4, 1, 16),
        ("sftp.speedLimitKBps", 0, 0, 10_000_000),
        ("reconnect.maxAttempts", 5, 1, 20),
        ("reconnect.baseDelayMs", 1000, 500, 10_000),
        ("reconnect.maxDelayMs", 15_000, 5_000, 60_000),
        ("connectionPool.idleTimeoutSecs", 1800, 0, 86_400),
        (
            "ai.toolUse.maxRounds",
            DEFAULT_AI_TOOL_MAX_ROUNDS,
            MIN_AI_TOOL_MAX_ROUNDS,
            MAX_AI_TOOL_MAX_ROUNDS,
        ),
        (
            "ai.toolUse.maxCallsPerRound",
            DEFAULT_AI_TOOL_MAX_CALLS_PER_ROUND,
            MIN_AI_TOOL_MAX_CALLS_PER_ROUND,
            MAX_AI_TOOL_MAX_CALLS_PER_ROUND,
        ),
        (
            "terminal.inBandTransfer.maxChunkBytes",
            1024 * 1024,
            64 * 1024,
            8 * 1024 * 1024,
        ),
        ("terminal.inBandTransfer.maxFileCount", 1024, 1, 10_000),
        (
            "terminal.inBandTransfer.maxTotalBytes",
            10 * 1024 * 1024 * 1024,
            100 * 1024 * 1024,
            100 * 1024 * 1024 * 1024,
        ),
        ("terminal.sessionLog.retentionDays", 30, 0, 3650),
        ("terminal.sessionLog.maxFileSizeMib", 100, 1, 4096),
    ] {
        let segments: Vec<_> = path.split('.').collect();
        if let Some(value) = get_path_mut(&mut settings, &segments) {
            clamp_i64(value, fallback, min, max, path, &mut validation_warnings);
        }
    }

    for (path, fallback, validator) in [
        (
            "terminal.sessionLog.fileNameTemplate",
            "{date}_{time}_{protocol}_{session}.log",
            parse_terminal_session_log_file_name_template
                as fn(
                    &str,
                ) -> std::result::Result<
                    ParsedTerminalSessionLogTemplate,
                    TerminalSessionLogTemplateError,
                >,
        ),
        (
            "terminal.sessionLog.contentTemplate",
            "[{timestamp}] {text}",
            parse_terminal_session_log_content_template
                as fn(
                    &str,
                ) -> std::result::Result<
                    ParsedTerminalSessionLogTemplate,
                    TerminalSessionLogTemplateError,
                >,
        ),
    ] {
        let segments: Vec<_> = path.split('.').collect();
        if let Some(value) = get_path_mut(&mut settings, &segments)
            && value
                .as_str()
                .is_none_or(|template| validator(template).is_err())
        {
            *value = json!(fallback);
            validation_warnings.push(format!("Reset invalid {path}"));
        }
    }

    for (path, fallback, min, max) in [
        ("terminal.lineHeight", 1.2, 0.8, 3.0),
        (
            "terminal.backgroundOpacity",
            DEFAULT_TERMINAL_BACKGROUND_OPACITY,
            MIN_TERMINAL_BACKGROUND_OPACITY,
            MAX_TERMINAL_BACKGROUND_OPACITY,
        ),
        (
            "appearance.windowOpacity",
            DEFAULT_WINDOW_OPACITY,
            MIN_WINDOW_OPACITY,
            MAX_WINDOW_OPACITY,
        ),
    ] {
        let segments: Vec<_> = path.split('.').collect();
        if let Some(value) = get_path_mut(&mut settings, &segments) {
            clamp_f64(value, fallback, min, max, path, &mut validation_warnings);
        }
    }

    sanitize_enum(
        &mut settings,
        &["general", "language"],
        &[
            "zh-CN", "en", "fr-FR", "ja", "es-ES", "pt-BR", "vi", "ko", "de", "it", "zh-TW",
        ],
        "zh-CN",
        &mut validation_warnings,
    );
    // Retired channels fall back to the channel appropriate for this build so
    // shared settings from older installations remain loadable.
    sanitize_enum(
        &mut settings,
        &["general", "updateChannel"],
        &["stable", "beta"],
        match UpdateChannel::default() {
            UpdateChannel::Stable => "stable",
            UpdateChannel::Beta => "beta",
        },
        &mut validation_warnings,
    );
    sanitize_enum(
        &mut settings,
        &["terminal", "fontFamily"],
        &[
            "jetbrains",
            "meslo",
            "maple",
            "cascadia",
            "consolas",
            "menlo",
            "custom",
        ],
        "jetbrains",
        &mut validation_warnings,
    );
    sanitize_enum(
        &mut settings,
        &["terminal", "cursorStyle"],
        &["block", "underline", "bar"],
        "block",
        &mut validation_warnings,
    );
    sanitize_enum(
        &mut settings,
        &["terminal", "renderer"],
        &["auto", "webgl", "canvas"],
        if cfg!(windows) { "canvas" } else { "auto" },
        &mut validation_warnings,
    );
    sanitize_enum(
        &mut settings,
        &["terminal", "sessionLog", "fileMode"],
        &["unique", "append", "overwrite"],
        "unique",
        &mut validation_warnings,
    );
    sanitize_enum(
        &mut settings,
        &["terminal", "terminalEncoding"],
        &[
            "utf-8",
            "gbk",
            "gb18030",
            "big5",
            "shift_jis",
            "euc-jp",
            "euc-kr",
            "windows-1252",
        ],
        "utf-8",
        &mut validation_warnings,
    );
    sanitize_enum(
        &mut settings,
        &["terminal", "backspaceSequence"],
        &["delete", "controlH"],
        "delete",
        &mut validation_warnings,
    );
    sanitize_enum(
        &mut settings,
        &["terminal", "deleteSequence"],
        &["csi3Tilde", "delete", "controlH"],
        "csi3Tilde",
        &mut validation_warnings,
    );
    sanitize_enum(
        &mut settings,
        &["terminal", "adaptiveRenderer"],
        &["auto", "always-60", "off"],
        "auto",
        &mut validation_warnings,
    );
    sanitize_enum(
        &mut settings,
        &["terminal", "backgroundFit"],
        &["cover", "contain", "fill", "tile"],
        "cover",
        &mut validation_warnings,
    );
    sanitize_enum(
        &mut settings,
        &["appearance", "uiDensity"],
        &["compact", "comfortable", "spacious"],
        "comfortable",
        &mut validation_warnings,
    );
    sanitize_enum(
        &mut settings,
        &["appearance", "animationSpeed"],
        &["off", "reduced", "normal", "fast"],
        "normal",
        &mut validation_warnings,
    );
    sanitize_enum(
        &mut settings,
        &["appearance", "frostedGlass"],
        &["off", "native", "system", "mica", "acrylic"],
        "off",
        &mut validation_warnings,
    );
    sanitize_enum(
        &mut settings,
        &["appearance", "renderProfile"],
        &["auto", "quality", "low-power", "compatibility"],
        "auto",
        &mut validation_warnings,
    );
    sanitize_enum(
        &mut settings,
        &["sftp", "conflictAction"],
        &["ask", "overwrite", "skip", "rename"],
        "ask",
        &mut validation_warnings,
    );
    sanitize_enum(
        &mut settings,
        &["ide", "agentMode"],
        &["ask", "enabled", "disabled"],
        "ask",
        &mut validation_warnings,
    );
    sanitize_enum(
        &mut settings,
        &["ai", "thinkingStyle"],
        &["detailed", "compact"],
        "detailed",
        &mut validation_warnings,
    );
    sanitize_enum(
        &mut settings,
        &["ai", "activeBackend"],
        &["provider", "acp"],
        "provider",
        &mut validation_warnings,
    );
    normalize_ai_reasoning_effort_aliases(&mut settings);
    sanitize_enum(
        &mut settings,
        &["ai", "reasoningEffort"],
        &[
            "none", "minimal", "low", "medium", "high", "xhigh", "max", "auto",
        ],
        "auto",
        &mut validation_warnings,
    );

    if let Some(terminal) = object_mut(&mut settings, "terminal")
        && let Some(in_band) = terminal
            .get_mut("inBandTransfer")
            .and_then(Value::as_object_mut)
    {
        in_band.insert("provider".to_string(), json!("trzsz"));
    }

    if let Some(value) = get_path_mut(&mut settings, &["terminal", "highlightRules"]) {
        *value = sanitize_highlight_rules_value(value);
    }
    sanitize_highlight_rule_sets(&mut settings, &mut validation_warnings);
    sanitize_custom_semantic_schemes(&mut settings, &mut validation_warnings);

    let settings =
        serde_json::from_value(settings).context("sanitized settings did not match schema")?;
    Ok(SanitizedSettings {
        settings,
        migration_warnings,
        validation_warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_custom_semantic_schemes_are_removed_before_deserialization() {
        let sanitized = sanitize_settings_value(json!({
            "terminal": {
                "semanticCustomScheme": "custom:invalid",
                "customSemanticSchemes": [{
                    "version": 1,
                    "id": "custom:invalid",
                    "name": "Invalid",
                    "rules": [{
                        "id": "bad-regex",
                        "enabled": true,
                        "pattern": "(",
                        "capture": 0,
                        "class": "error",
                        "priority": 80,
                        "context": "any"
                    }]
                }]
            }
        }))
        .expect("sanitize settings");

        assert!(
            sanitized
                .settings
                .terminal
                .custom_semantic_schemes
                .is_empty()
        );
        assert!(sanitized.settings.terminal.semantic_custom_scheme.is_none());
        assert!(!sanitized.validation_warnings.is_empty());
    }

    #[test]
    fn invalid_session_log_templates_and_mode_fall_back_to_safe_defaults() {
        let sanitized = sanitize_settings_value(json!({
            "terminal": {
                "sessionLog": {
                    "fileNameTemplate": "../escape.log",
                    "contentTemplate": "missing text variable",
                    "fileMode": "replaceAnything"
                }
            }
        }))
        .expect("sanitize settings");

        let session_log = sanitized.settings.terminal.session_log;
        assert_eq!(
            session_log.file_name_template,
            "{date}_{time}_{protocol}_{session}.log"
        );
        assert_eq!(session_log.content_template, "[{timestamp}] {text}");
        assert_eq!(session_log.file_mode, TerminalSessionLogFileMode::Unique);
        assert_eq!(sanitized.validation_warnings.len(), 3);
    }

    #[test]
    fn missing_default_highlight_rule_set_falls_back_to_global_base() {
        let sanitized = sanitize_settings_value(json!({
            "terminal": {
                "defaultHighlightRuleSet": "missing",
                "highlightRuleSets": [{
                    "id": "operations",
                    "name": "Operations",
                    "rules": []
                }]
            }
        }))
        .expect("sanitize settings");

        assert!(
            sanitized
                .settings
                .terminal
                .default_highlight_rule_set
                .is_none()
        );
        assert_eq!(sanitized.settings.terminal.highlight_rule_sets.len(), 1);
    }

    #[test]
    fn local_shell_scheme_bindings_keep_builtins_and_remove_missing_custom_schemes() {
        let sanitized = sanitize_settings_value(json!({
            "localTerminal": {
                "semanticSchemeByShell": {
                    "bash": "conservative",
                    "zsh": "custom:missing"
                }
            }
        }))
        .expect("sanitize settings");

        assert_eq!(
            sanitized
                .settings
                .local_terminal
                .semantic_scheme_for_shell("bash"),
            Some("conservative")
        );
        assert!(
            sanitized
                .settings
                .local_terminal
                .semantic_scheme_for_shell("zsh")
                .is_none()
        );
    }

    #[test]
    fn retired_gpui_preview_channel_migrates_to_the_build_default() {
        let sanitized = sanitize_settings_value(json!({
            "general": { "updateChannel": "gpui-preview" }
        }))
        .expect("sanitize retired update channel");

        assert_eq!(
            sanitized.settings.general.update_channel,
            UpdateChannel::default()
        );
        assert!(
            sanitized
                .validation_warnings
                .iter()
                .any(|warning| warning.contains("general.updateChannel"))
        );
    }

    #[test]
    fn legacy_ai_memory_migrates_once_to_an_itemized_entry() {
        let sanitized = sanitize_settings_value(json!({
            "ai": {
                "memory": {
                    "enabled": true,
                    "content": "Prefer concise terminal explanations."
                }
            }
        }))
        .expect("sanitize settings");

        assert_eq!(sanitized.settings.ai.memory.entries.len(), 1);
        let entry = &sanitized.settings.ai.memory.entries[0];
        assert_eq!(entry.id, "legacy-user-memory");
        assert_eq!(entry.scope_kind, AiMemoryScopeKind::User);
        assert_eq!(entry.memory_kind, AiMemoryKind::LongTerm);
        assert_eq!(entry.source, AiMemorySource::Migrated);
        assert_eq!(entry.revision, 1);
    }

    #[test]
    fn appearance_matrix_values_survive_sanitization() {
        for density in ["compact", "comfortable", "spacious"] {
            for frosted_glass in ["off", "native", "system", "mica", "acrylic"] {
                let sanitized = sanitize_settings_value(json!({
                    "appearance": {
                        "uiDensity": density,
                        "animationSpeed": "off",
                        "borderRadius": 16,
                        "frostedGlass": frosted_glass
                    },
                    "terminal": {
                        "backgroundBlur": 20,
                        "backgroundOpacity": 0.15
                    }
                }))
                .expect("sanitize appearance matrix settings");

                assert_eq!(
                    serde_json::to_value(sanitized.settings.appearance.ui_density)
                        .expect("serialize density"),
                    json!(density)
                );
                assert_eq!(sanitized.settings.appearance.border_radius, 16);
                assert_eq!(sanitized.settings.terminal.background_blur, 20);
            }
        }
    }

    #[test]
    fn background_opacity_accepts_full_visibility_and_clamps_oversized_values() {
        let full_visibility = sanitize_settings_value(json!({
            "terminal": { "backgroundOpacity": 1.0 }
        }))
        .expect("sanitize full background opacity");
        assert_eq!(
            full_visibility.settings.terminal.background_opacity,
            MAX_TERMINAL_BACKGROUND_OPACITY
        );
        assert!(full_visibility.validation_warnings.is_empty());

        let oversized = sanitize_settings_value(json!({
            "terminal": { "backgroundOpacity": 1.5 }
        }))
        .expect("sanitize oversized background opacity");
        assert_eq!(
            oversized.settings.terminal.background_opacity,
            MAX_TERMINAL_BACKGROUND_OPACITY
        );
        assert!(
            oversized
                .validation_warnings
                .iter()
                .any(|warning| warning.contains("terminal.backgroundOpacity"))
        );
    }

    #[test]
    fn window_opacity_defaults_to_opaque_and_clamps_unreadable_values() {
        let legacy = sanitize_settings_value(json!({
            "appearance": {}
        }))
        .expect("sanitize settings without window opacity");
        assert_eq!(
            legacy.settings.appearance.window_opacity,
            DEFAULT_WINDOW_OPACITY
        );

        let too_transparent = sanitize_settings_value(json!({
            "appearance": { "windowOpacity": 0.1 }
        }))
        .expect("sanitize overly transparent window opacity");
        assert_eq!(
            too_transparent.settings.appearance.window_opacity,
            MIN_WINDOW_OPACITY
        );
        assert!(
            too_transparent
                .validation_warnings
                .iter()
                .any(|warning| warning.contains("appearance.windowOpacity"))
        );
    }

    #[test]
    fn ui_font_size_defaults_and_clamps_during_sanitization() {
        let legacy = sanitize_settings_value(json!({
            "appearance": {}
        }))
        .expect("sanitize settings without a UI font size");
        assert_eq!(
            legacy.settings.appearance.ui_font_size,
            DEFAULT_UI_FONT_SIZE
        );

        let oversized = sanitize_settings_value(json!({
            "appearance": { "uiFontSize": 100 }
        }))
        .expect("sanitize oversized UI font size");
        assert_eq!(oversized.settings.appearance.ui_font_size, 20);
        assert!(
            oversized
                .validation_warnings
                .iter()
                .any(|warning| warning.contains("appearance.uiFontSize"))
        );
    }

    #[test]
    fn legacy_css_frosted_glass_falls_back_to_off() {
        let sanitized = sanitize_settings_value(json!({
            "appearance": { "frostedGlass": "css" }
        }))
        .expect("sanitize legacy frosted glass setting");

        assert_eq!(
            sanitized.settings.appearance.frosted_glass,
            crate::FrostedGlassMode::Off
        );
        assert!(!sanitized.validation_warnings.is_empty());
    }

    #[test]
    fn removes_legacy_provider_default_model_without_selecting_it() {
        let sanitized = sanitize_settings_value(json!({
            "ai": {
                "providers": [{
                    "id": "provider-1",
                    "type": "openai",
                    "name": "OpenAI",
                    "baseUrl": "https://api.openai.com/v1",
                    "defaultModel": "gpt-4o-mini",
                    "models": ["gpt-4o-mini"],
                    "enabled": true
                }],
                "activeProviderId": "provider-1",
                "activeModel": null
            }
        }))
        .expect("sanitize settings");

        assert_eq!(sanitized.settings.ai.active_model, None);
        assert!(
            sanitized.settings.ai.providers[0]
                .get("defaultModel")
                .is_none()
        );
    }

    #[test]
    fn accepts_legacy_native_sftp_speed_limit_key() {
        let sanitized = sanitize_settings_value(json!({
            "sftp": {
                "speedLimitEnabled": true,
                "speedLimitKbps": 2048
            }
        }))
        .expect("sanitize settings");

        assert!(sanitized.settings.sftp.speed_limit_enabled);
        assert_eq!(sanitized.settings.sftp.speed_limit_kbps, 2048);
        assert!(!sanitized.settings.sftp.extra.contains_key("speedLimitKbps"));
    }

    #[test]
    fn migrates_legacy_acp_agent_presets_to_current_launch_commands() {
        let sanitized = sanitize_settings_value(json!({
            "ai": {
                "acpAgents": [
                    {
                        "id": "claude-code",
                        "displayName": "Claude Code",
                        "command": "npx",
                        "args": ["-y", "@agentclientprotocol/claude-agent-acp"]
                    },
                    {
                        "id": "codex",
                        "displayName": "Codex",
                        "command": "codex-acp",
                        "args": []
                    },
                    {
                        "id": "custom",
                        "displayName": "Custom",
                        "command": "custom-acp",
                        "args": ["--stdio"]
                    },
                    {
                        "id": "github-copilot",
                        "displayName": "GitHub Copilot",
                        "command": "oxideterm-native",
                        "args": ["--acp-adapter", "github-copilot"]
                    }
                ]
            }
        }))
        .expect("sanitize settings");

        let agents = sanitized.settings.ai.acp_agents;
        assert_eq!(agents[0].command, "oxideterm-native");
        assert_eq!(agents[0].args, vec!["--acp-adapter", "claude-code"]);
        assert_eq!(agents[1].command, "oxideterm-native");
        assert_eq!(agents[1].args, vec!["--acp-adapter", "codex"]);
        assert_eq!(agents[2].command, "custom-acp");
        assert_eq!(agents[2].args, vec!["--stdio"]);
        assert_eq!(agents[3].command, "copilot");
        assert_eq!(agents[3].args, vec!["--acp", "--stdio"]);
    }

    #[test]
    fn explicit_write_resource_subscope_auto_approval_is_preserved() {
        let sanitized = sanitize_settings_value(json!({
            "ai": {
                "toolUse": {
                    "autoApproveTools": {
                        "write_resource": true,
                        "write_resource:file": false
                    }
                }
            }
        }))
        .expect("sanitize settings");

        let auto_approve = sanitized.settings.ai.tool_use.auto_approve_tools;
        assert_eq!(
            auto_approve
                .get("write_resource:settings")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            auto_approve
                .get("write_resource:file")
                .and_then(Value::as_bool),
            Some(false)
        );
    }

    #[test]
    fn existing_tool_policy_receives_new_skill_tool_defaults() {
        let sanitized = sanitize_settings_value(json!({
            "ai": {
                "toolUse": {
                    "autoApproveTools": {
                        "run_command": true
                    }
                }
            }
        }))
        .expect("sanitize settings");

        let policy = sanitized.settings.ai.tool_use.auto_approve_tools;
        assert_eq!(
            policy.get("run_command").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            policy.get(AI_TOOL_LOAD_SKILL).and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            policy
                .get(AI_TOOL_READ_SKILL_RESOURCE)
                .and_then(Value::as_bool),
            Some(true)
        );
    }

    #[test]
    fn missing_execution_profiles_keep_active_ai_settings() {
        let sanitized = sanitize_settings_value(json!({
            "ai": {
                "providers": [{
                    "id": "provider-1",
                    "type": "openai_compatible",
                    "name": "Provider 1",
                    "baseUrl": "https://gateway.example/v1",
                    "models": ["model-1"],
                    "enabled": true,
                    "createdAt": 1
                }],
                "activeProviderId": "provider-1",
                "activeModel": "model-1",
                "reasoningEffort": "high",
                "toolUse": {
                    "enabled": true,
                    "maxRounds": 12,
                    "maxCallsPerRound": 6,
                    "autoApproveTools": { "read_resource": true },
                    "disabledTools": ["run_command"]
                }
            }
        }))
        .expect("sanitize settings");

        let ai = sanitized.settings.ai;
        assert_eq!(ai.active_provider_id.as_deref(), Some("provider-1"));
        assert_eq!(ai.active_model.as_deref(), Some("model-1"));
        assert_eq!(ai.reasoning_effort, AiReasoningEffort::High);
        assert!(ai.tool_use.enabled);
        assert_eq!(ai.tool_use.max_rounds, Some(12));
        assert_eq!(
            ai.tool_use.disabled_tools.first().map(String::as_str),
            Some("run_command"),
        );
        assert!(!ai.extra.contains_key("executionProfiles"));
    }

    #[test]
    fn ai_tool_policy_defaults_fill_new_capabilities_without_overwriting_saved_choices() {
        let sanitized = sanitize_settings_value(json!({
            "ai": {
                "toolUse": {
                    "autoApproveTools": {
                        "run_command": true
                    }
                }
            }
        }))
        .expect("sanitize settings");
        let policy = &sanitized.settings.ai.tool_use.auto_approve_tools;

        assert_eq!(
            policy.get("run_command").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            policy
                .get(AI_TOOL_LIST_BACKGROUND_TASKS)
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            policy
                .get(AI_TOOL_CREATE_BACKGROUND_TASK)
                .and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            policy
                .get(AI_TOOL_CONTROL_HOST_TOOL)
                .and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            policy.get(AI_TOOL_MANAGE_FORWARD).and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            policy.get(AI_TOOL_MANAGE_PLUGIN).and_then(Value::as_bool),
            Some(false)
        );
    }

    #[test]
    fn legacy_native_profile_reasoning_aliases_migrate_to_top_level() {
        let sanitized = sanitize_settings_value(json!({
            "ai": {
                "providers": [{
                    "id": "provider-1",
                    "type": "openai",
                    "name": "OpenAI",
                    "baseUrl": "https://api.openai.com/v1",
                    "models": ["gpt-4o-mini"],
                    "enabled": true,
                    "createdAt": 1
                }],
                "activeProviderId": "provider-1",
                "activeModel": "gpt-4o-mini",
                "executionProfiles": {
                    "defaultProfileId": "default",
                    "profiles": [{
                        "id": "default",
                        "name": "Default",
                        "providerId": "provider-1",
                        "model": "gpt-4o-mini",
                        "reasoningEffort": "xhigh"
                    }]
                }
            }
        }))
        .expect("sanitize settings");

        assert_eq!(
            sanitized.settings.ai.reasoning_effort,
            AiReasoningEffort::Xhigh
        );
        assert_eq!(
            sanitized.settings.ai.active_provider_id.as_deref(),
            Some("provider-1")
        );
        assert_eq!(
            sanitized.settings.ai.active_model.as_deref(),
            Some("gpt-4o-mini")
        );
        assert!(
            !sanitized
                .settings
                .ai
                .extra
                .contains_key("executionProfiles")
        );
    }

    #[test]
    fn legacy_execution_profiles_with_missing_default_migrate_first_profile() {
        let sanitized = sanitize_settings_value(json!({
            "ai": {
                "executionProfiles": {
                    "defaultProfileId": "missing",
                    "profiles": [{
                        "id": "first",
                        "name": "First",
                        "providerId": "provider-1",
                        "model": "model-1",
                        "reasoningEffort": "auto",
                        "createdAt": 1,
                        "updatedAt": 1
                    }]
                }
            }
        }))
        .expect("sanitize settings");

        assert_eq!(
            sanitized.settings.ai.active_provider_id.as_deref(),
            Some("provider-1")
        );
        assert_eq!(
            sanitized.settings.ai.active_model.as_deref(),
            Some("model-1")
        );
        assert!(
            !sanitized
                .settings
                .ai
                .extra
                .contains_key("executionProfiles")
        );
    }

    #[test]
    fn legacy_acp_execution_profile_migrates_to_active_agent() {
        let sanitized = sanitize_settings_value(json!({
            "ai": {
                "executionProfiles": {
                    "defaultProfileId": "codex-profile",
                    "profiles": [{
                        "id": "codex-profile",
                        "backend": "acp",
                        "acpAgentId": "codex",
                        "reasoningEffort": "off"
                    }]
                }
            }
        }))
        .expect("sanitize settings");

        assert_eq!(sanitized.settings.ai.active_backend, AiActiveBackend::Acp);
        assert_eq!(
            sanitized.settings.ai.active_acp_agent_id.as_deref(),
            Some("codex")
        );
        assert_eq!(
            sanitized.settings.ai.reasoning_effort,
            AiReasoningEffort::None
        );
        assert!(
            !sanitized
                .settings
                .ai
                .extra
                .contains_key("executionProfiles")
        );
    }

    #[test]
    fn migrates_legacy_custom_ai_base_url_first() {
        let sanitized = sanitize_settings_value(json!({
            "ai": {
                "baseUrl": "https://gateway.example/v1",
                "model": "gateway-model",
                "providers": []
            }
        }))
        .expect("sanitize settings");

        let first = sanitized
            .settings
            .ai
            .providers
            .first()
            .expect("first provider");
        assert_eq!(
            first.get("type").and_then(Value::as_str),
            Some("openai_compatible")
        );
        assert_eq!(
            first.get("baseUrl").and_then(Value::as_str),
            Some("https://gateway.example/v1")
        );
        assert_eq!(
            first
                .get("models")
                .and_then(Value::as_array)
                .and_then(|models| models.first())
                .and_then(Value::as_str),
            Some("gateway-model")
        );
        assert_eq!(
            sanitized.settings.ai.active_provider_id.as_deref(),
            first.get("id").and_then(Value::as_str)
        );
        assert_eq!(sanitized.settings.ai.active_model, None);
    }
}
