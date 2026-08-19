// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! AI settings page model helpers.

use std::{
    collections::{BTreeMap, HashSet, hash_map::DefaultHasher},
    fmt,
    hash::{Hash, Hasher},
};

use oxideterm_ai::{
    AiProviderView, ContextWindowSource, McpAuthHeaderMode, McpTransport, ProviderModelRefresh,
    model_context_window_info, provider_views as ai_provider_views_from_values,
    update_provider as ai_update_provider_values,
};
use oxideterm_settings::{
    AI_TOOL_CANCEL_BACKGROUND_TASK, AI_TOOL_CONFIGURE_CLOUD_SYNC, AI_TOOL_CONTROL_HOST_TOOL,
    AI_TOOL_CREATE_BACKGROUND_TASK, AI_TOOL_GET_BACKGROUND_TASK, AI_TOOL_GET_CLOUD_SYNC_STATE,
    AI_TOOL_GET_TRANSPORT_SESSION_STATE, AI_TOOL_INSPECT_HOST_TOOLS, AI_TOOL_LIST_BACKGROUND_TASKS,
    AI_TOOL_LIST_CREDENTIALS, AI_TOOL_LIST_FORWARDS, AI_TOOL_LIST_PLUGINS,
    AI_TOOL_LIST_REMOTE_DESKTOP_SESSIONS, AI_TOOL_LIST_TRANSPORT_PROFILES, AI_TOOL_LOAD_SKILL,
    AI_TOOL_MANAGE_CLOUD_SYNC, AI_TOOL_MANAGE_CREDENTIAL, AI_TOOL_MANAGE_FORWARD,
    AI_TOOL_MANAGE_PLUGIN, AI_TOOL_MANAGE_REMOTE_DESKTOP_SESSION, AI_TOOL_MANAGE_SERIAL_SESSION,
    AI_TOOL_MANAGE_TELNET_SESSION, AI_TOOL_OPEN_TRANSPORT_PROFILE, AI_TOOL_READ_SKILL_RESOURCE,
    AcpAgentAuthState, AcpAgentCapabilityPolicy, AcpAgentConfig, AcpAgentRuntimeStatus,
    PersistedSettings,
};
use zeroize::{Zeroize, ZeroizeOnDrop};

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

/// Aggregate auto-approval state for one policy category.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiToolPolicyGroupState {
    Locked,
    NoneApproved,
    PartiallyApproved,
    AllApproved,
}

impl AiToolPolicyGroup {
    /// Derives the aggregate state from mutable policy items only.
    pub fn state(&self) -> AiToolPolicyGroupState {
        let (mutable_count, approved_count) = self
            .items
            .iter()
            .filter(|item| !item.locked && item.key.is_some())
            .fold((0_usize, 0_usize), |(total, approved), item| {
                (total + 1, approved + usize::from(item.checked))
            });
        if mutable_count == 0 {
            return AiToolPolicyGroupState::Locked;
        }

        if approved_count == 0 {
            AiToolPolicyGroupState::NoneApproved
        } else if approved_count == mutable_count {
            AiToolPolicyGroupState::AllApproved
        } else {
            AiToolPolicyGroupState::PartiallyApproved
        }
    }

    /// Returns the value applied by the next category-level toggle.
    pub fn next_bulk_value(&self) -> Option<bool> {
        match self.state() {
            AiToolPolicyGroupState::Locked => None,
            AiToolPolicyGroupState::AllApproved => Some(false),
            AiToolPolicyGroupState::NoneApproved | AiToolPolicyGroupState::PartiallyApproved => {
                Some(true)
            }
        }
    }
}

/// Applies one auto-approval value to every mutable item in a category.
pub fn set_ai_tool_policy_group_approval(
    settings: &mut PersistedSettings,
    group: &AiToolPolicyGroup,
    approved: bool,
) {
    // Locked discovery tools are system policy and must never be changed by a
    // group-level convenience control.
    for item in group.items.iter().filter(|item| !item.locked) {
        if let Some(key) = item.key {
            settings
                .ai
                .tool_use
                .auto_approve_tools
                .insert(key.to_string(), serde_json::json!(approved));
        }
    }
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
pub struct AiModelContextWindowRow {
    pub has_override: bool,
    pub source: ContextWindowSource,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AcpAgentPreset {
    ClaudeCode,
    Codex,
    GeminiCli,
    GithubCopilot,
    OpenCode,
}

struct AcpAgentPresetTemplate {
    base_id: &'static str,
    display_name: &'static str,
    command: &'static str,
    args: &'static [&'static str],
}

impl AcpAgentPreset {
    pub fn display_name(self) -> &'static str {
        self.template().display_name
    }

    fn base_id(self) -> &'static str {
        self.template().base_id
    }

    fn template(self) -> AcpAgentPresetTemplate {
        match self {
            Self::ClaudeCode => AcpAgentPresetTemplate {
                base_id: "claude-code",
                display_name: "Claude Code",
                command: "oxideterm-native",
                args: &["--acp-adapter", "claude-code"],
            },
            Self::Codex => AcpAgentPresetTemplate {
                base_id: "codex",
                display_name: "Codex",
                command: "oxideterm-native",
                args: &["--acp-adapter", "codex"],
            },
            Self::GeminiCli => AcpAgentPresetTemplate {
                base_id: "gemini-cli",
                display_name: "Gemini CLI",
                command: "gemini",
                // Gemini CLI exposes its native ACP server over stdio.
                args: &["--acp"],
            },
            Self::GithubCopilot => AcpAgentPresetTemplate {
                base_id: "github-copilot",
                display_name: "GitHub Copilot",
                command: "copilot",
                // GitHub Copilot CLI exposes a native ACP stdio server.
                args: &["--acp", "--stdio"],
            },
            Self::OpenCode => AcpAgentPresetTemplate {
                base_id: "opencode",
                display_name: "OpenCode",
                command: "opencode",
                // OpenCode exposes its native ACP server over stdio.
                args: &["acp"],
            },
        }
    }
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
            items: vec![
                AiToolPolicyItem {
                    key: None,
                    label_key: "settings_view.ai.tool_policy_read_auto",
                    checked: true,
                    locked: true,
                },
                AiToolPolicyItem {
                    key: Some(AI_TOOL_LIST_BACKGROUND_TASKS),
                    label_key: "settings_view.ai.tool_policy_read_background_tasks",
                    checked: true,
                    locked: true,
                },
                AiToolPolicyItem {
                    key: Some(AI_TOOL_GET_BACKGROUND_TASK),
                    label_key: "settings_view.ai.tool_policy_read_background_task_details",
                    checked: true,
                    locked: true,
                },
                AiToolPolicyItem {
                    key: Some(AI_TOOL_INSPECT_HOST_TOOLS),
                    label_key: "settings_view.ai.tool_policy_read_host_tools",
                    checked: true,
                    locked: true,
                },
                AiToolPolicyItem {
                    key: Some(AI_TOOL_LIST_FORWARDS),
                    label_key: "settings_view.ai.tool_policy_read_forwards",
                    checked: true,
                    locked: true,
                },
                AiToolPolicyItem {
                    key: Some(AI_TOOL_LIST_PLUGINS),
                    label_key: "settings_view.ai.tool_policy_read_plugins",
                    checked: true,
                    locked: true,
                },
                AiToolPolicyItem {
                    key: Some(AI_TOOL_LIST_TRANSPORT_PROFILES),
                    label_key: "settings_view.ai.tool_policy_read_transport_profiles",
                    checked: true,
                    locked: true,
                },
                AiToolPolicyItem {
                    key: Some(AI_TOOL_GET_TRANSPORT_SESSION_STATE),
                    label_key: "settings_view.ai.tool_policy_read_transport_session_state",
                    checked: true,
                    locked: true,
                },
                AiToolPolicyItem {
                    key: Some(AI_TOOL_LIST_REMOTE_DESKTOP_SESSIONS),
                    label_key: "settings_view.ai.tool_policy_read_remote_desktop_sessions",
                    checked: true,
                    locked: true,
                },
                AiToolPolicyItem {
                    key: Some(AI_TOOL_GET_CLOUD_SYNC_STATE),
                    label_key: "settings_view.ai.tool_policy_read_cloud_sync",
                    checked: true,
                    locked: true,
                },
                AiToolPolicyItem {
                    key: Some(AI_TOOL_LIST_CREDENTIALS),
                    label_key: "settings_view.ai.tool_policy_read_credentials",
                    checked: true,
                    locked: true,
                },
                AiToolPolicyItem {
                    key: Some("list_memory_entries"),
                    label_key: "settings_view.ai.tool_policy_read_memory_entries",
                    checked: true,
                    locked: true,
                },
                AiToolPolicyItem {
                    key: Some(AI_TOOL_LOAD_SKILL),
                    label_key: "settings_view.ai.tool_policy_read_load_skill",
                    checked: true,
                    locked: true,
                },
                AiToolPolicyItem {
                    key: Some(AI_TOOL_READ_SKILL_RESOURCE),
                    label_key: "settings_view.ai.tool_policy_read_skill_resource",
                    checked: true,
                    locked: true,
                },
            ],
        },
        AiToolPolicyGroup {
            title_key: "settings_view.ai.tool_policy_execute_title",
            description_key: "settings_view.ai.tool_policy_execute_desc",
            items: vec![
                AiToolPolicyItem {
                    key: Some("run_command"),
                    label_key: "settings_view.ai.tool_policy_execute_run_command",
                    checked: checked("run_command"),
                    locked: false,
                },
                AiToolPolicyItem {
                    key: Some("send_terminal_input"),
                    label_key: "settings_view.ai.tool_policy_interactive_send_input",
                    checked: checked("send_terminal_input"),
                    locked: false,
                },
                AiToolPolicyItem {
                    key: Some("wait_terminal_output"),
                    label_key: "settings_view.ai.tool_policy_wait_terminal_output",
                    checked: true,
                    locked: true,
                },
                AiToolPolicyItem {
                    key: Some("get_terminal_command_status"),
                    label_key: "settings_view.ai.tool_policy_terminal_command_status",
                    checked: true,
                    locked: true,
                },
                AiToolPolicyItem {
                    key: Some(AI_TOOL_MANAGE_SERIAL_SESSION),
                    label_key: "settings_view.ai.tool_policy_manage_serial_session",
                    checked: checked(AI_TOOL_MANAGE_SERIAL_SESSION),
                    locked: false,
                },
                AiToolPolicyItem {
                    key: Some(AI_TOOL_MANAGE_TELNET_SESSION),
                    label_key: "settings_view.ai.tool_policy_manage_telnet_session",
                    checked: checked(AI_TOOL_MANAGE_TELNET_SESSION),
                    locked: false,
                },
            ],
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
                AiToolPolicyItem {
                    key: Some(AI_TOOL_OPEN_TRANSPORT_PROFILE),
                    label_key: "settings_view.ai.tool_policy_open_transport_profile",
                    checked: checked(AI_TOOL_OPEN_TRANSPORT_PROFILE),
                    locked: false,
                },
                AiToolPolicyItem {
                    key: Some("manage_memory_entry"),
                    label_key: "settings_view.ai.tool_policy_manage_memory_entry",
                    checked: checked("manage_memory_entry"),
                    locked: false,
                },
            ],
        },
        AiToolPolicyGroup {
            title_key: "settings_view.ai.tool_policy_background_title",
            description_key: "settings_view.ai.tool_policy_background_desc",
            items: vec![
                AiToolPolicyItem {
                    key: Some(AI_TOOL_CREATE_BACKGROUND_TASK),
                    label_key: "settings_view.ai.tool_policy_create_background_task",
                    checked: checked(AI_TOOL_CREATE_BACKGROUND_TASK),
                    locked: false,
                },
                AiToolPolicyItem {
                    key: Some(AI_TOOL_CANCEL_BACKGROUND_TASK),
                    label_key: "settings_view.ai.tool_policy_cancel_background_task",
                    checked: checked(AI_TOOL_CANCEL_BACKGROUND_TASK),
                    locked: false,
                },
            ],
        },
        AiToolPolicyGroup {
            title_key: "settings_view.ai.tool_policy_operations_title",
            description_key: "settings_view.ai.tool_policy_operations_desc",
            items: vec![
                AiToolPolicyItem {
                    key: Some(AI_TOOL_CONTROL_HOST_TOOL),
                    label_key: "settings_view.ai.tool_policy_control_host_tool",
                    checked: checked(AI_TOOL_CONTROL_HOST_TOOL),
                    locked: false,
                },
                AiToolPolicyItem {
                    key: Some(AI_TOOL_MANAGE_FORWARD),
                    label_key: "settings_view.ai.tool_policy_manage_forward",
                    checked: checked(AI_TOOL_MANAGE_FORWARD),
                    locked: false,
                },
                AiToolPolicyItem {
                    key: Some(AI_TOOL_MANAGE_PLUGIN),
                    label_key: "settings_view.ai.tool_policy_manage_plugin",
                    checked: checked(AI_TOOL_MANAGE_PLUGIN),
                    locked: false,
                },
                AiToolPolicyItem {
                    key: Some(AI_TOOL_MANAGE_REMOTE_DESKTOP_SESSION),
                    label_key: "settings_view.ai.tool_policy_manage_remote_desktop",
                    checked: checked(AI_TOOL_MANAGE_REMOTE_DESKTOP_SESSION),
                    locked: false,
                },
                AiToolPolicyItem {
                    key: Some(AI_TOOL_MANAGE_CLOUD_SYNC),
                    label_key: "settings_view.ai.tool_policy_manage_cloud_sync",
                    checked: checked(AI_TOOL_MANAGE_CLOUD_SYNC),
                    locked: false,
                },
                AiToolPolicyItem {
                    key: Some(AI_TOOL_CONFIGURE_CLOUD_SYNC),
                    label_key: "settings_view.ai.tool_policy_configure_cloud_sync",
                    checked: checked(AI_TOOL_CONFIGURE_CLOUD_SYNC),
                    locked: false,
                },
                AiToolPolicyItem {
                    key: Some(AI_TOOL_MANAGE_CREDENTIAL),
                    label_key: "settings_view.ai.tool_policy_manage_credentials",
                    checked: checked(AI_TOOL_MANAGE_CREDENTIAL),
                    locked: false,
                },
            ],
        },
    ]
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

pub fn ai_add_acp_agent(settings: &mut PersistedSettings) {
    let now = current_time_millis();
    settings.ai.acp_agents.push(AcpAgentConfig {
        id: format!("acp-agent-{now}"),
        display_name: String::new(),
        command: String::new(),
        args: Vec::new(),
        env: BTreeMap::new(),
        cwd: None,
        enabled: true,
        auth: AcpAgentAuthState::default(),
        capability_policy: AcpAgentCapabilityPolicy::default(),
        status: AcpAgentRuntimeStatus::default(),
    });
}

pub fn ai_add_acp_agent_preset(settings: &mut PersistedSettings, preset: AcpAgentPreset) {
    let id = ai_unique_acp_agent_id(settings, preset.base_id());
    let template = preset.template();
    // Presets seed editable agent entries; capabilities stay closed until users opt in.
    settings.ai.acp_agents.push(AcpAgentConfig {
        id,
        display_name: template.display_name.to_string(),
        command: template.command.to_string(),
        args: template.args.iter().map(|arg| (*arg).to_string()).collect(),
        env: BTreeMap::new(),
        cwd: None,
        enabled: true,
        auth: AcpAgentAuthState::default(),
        capability_policy: AcpAgentCapabilityPolicy::default(),
        status: AcpAgentRuntimeStatus::default(),
    });
}

fn ai_unique_acp_agent_id(settings: &PersistedSettings, base_id: &str) -> String {
    if !settings
        .ai
        .acp_agents
        .iter()
        .any(|agent| agent.id == base_id)
    {
        return base_id.to_string();
    }
    for suffix in 2usize.. {
        let candidate = format!("{base_id}-{suffix}");
        if !settings
            .ai
            .acp_agents
            .iter()
            .any(|agent| agent.id == candidate)
        {
            return candidate;
        }
    }
    unreachable!("unbounded suffix search should always find a free ACP agent id")
}

pub fn ai_delete_acp_agent(settings: &mut PersistedSettings, index: usize) {
    if index >= settings.ai.acp_agents.len() {
        return;
    }
    let removed_id = settings.ai.acp_agents.remove(index).id;
    if settings.ai.active_acp_agent_id.as_deref() == Some(removed_id.as_str()) {
        settings.ai.active_acp_agent_id =
            settings.ai.acp_agents.first().map(|agent| agent.id.clone());
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
    fn ai_mcp_draft_validation_rejects_duplicates_and_invalid_names() {
        let mut settings = PersistedSettings::default();
        settings.ai.mcp_servers.push(serde_json::json!({
            "id": "existing",
            "name": "existing",
            "transport": "stdio",
            "command": "node",
            "enabled": true
        }));

        let mut draft = AiMcpServerDraft::default();
        draft.name = "new-server".to_string();
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
    fn ai_mcp_draft_debug_redacts_secret_values() {
        let mut draft = AiMcpServerDraft::default();
        draft.command = "command-secret".to_string();
        draft.args = "positional-secret --api-key=arg-secret".to_string();
        draft.env = vec![("API_KEY".to_string(), "env-secret".to_string())];
        draft.url = "https://url-secret@example.test?token=query-secret".to_string();
        draft.auth_token = "auth-secret".to_string();
        draft.headers = vec![("Authorization".to_string(), "header-secret".to_string())];

        let debug = format!("{draft:?}");

        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("command-secret"));
        assert!(!debug.contains("positional-secret"));
        assert!(!debug.contains("arg-secret"));
        assert!(!debug.contains("env-secret"));
        assert!(!debug.contains("url-secret"));
        assert!(!debug.contains("query-secret"));
        assert!(!debug.contains("auth-secret"));
        assert!(!debug.contains("header-secret"));
    }

    #[test]
    fn ai_mcp_draft_zeroize_clears_all_secret_capable_fields() {
        let mut draft = AiMcpServerDraft::default();
        draft.args = "--token arg-secret".to_string();
        draft.env = vec![("API_KEY".to_string(), "env-secret".to_string())];
        draft.auth_token = "auth-secret".to_string();
        draft.headers = vec![("Authorization".to_string(), "header-secret".to_string())];

        draft.zeroize();

        assert!(draft.args.is_empty());
        assert!(draft.env.is_empty());
        assert!(draft.auth_token.is_empty());
        assert!(draft.headers.is_empty());
    }

    #[test]
    fn ai_mcp_draft_input_adapter_trims_identity_fields_only() {
        let mut draft = AiMcpServerDraft::default();
        draft.env = vec![(String::new(), String::new())];

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
    fn ai_tool_policy_group_bulk_toggle_preserves_locked_policy() {
        let mut settings = PersistedSettings::default();
        settings
            .ai
            .tool_use
            .auto_approve_tools
            .insert("run_command".to_string(), serde_json::json!(true));

        let groups = ai_tool_policy_groups(&settings);
        let locked_group = groups[0].clone();
        let terminal_group = groups[1].clone();
        assert_eq!(locked_group.state(), AiToolPolicyGroupState::Locked);
        assert_eq!(locked_group.next_bulk_value(), None);
        assert_eq!(
            terminal_group.state(),
            AiToolPolicyGroupState::PartiallyApproved
        );
        assert_eq!(terminal_group.next_bulk_value(), Some(true));

        let locked_policy_before = settings.ai.tool_use.auto_approve_tools.clone();
        set_ai_tool_policy_group_approval(&mut settings, &locked_group, false);
        assert_eq!(
            settings.ai.tool_use.auto_approve_tools,
            locked_policy_before
        );

        set_ai_tool_policy_group_approval(&mut settings, &terminal_group, true);
        let terminal_group = ai_tool_policy_groups(&settings)[1].clone();
        assert_eq!(terminal_group.state(), AiToolPolicyGroupState::AllApproved);
        assert_eq!(terminal_group.next_bulk_value(), Some(false));

        set_ai_tool_policy_group_approval(&mut settings, &terminal_group, false);
        assert_eq!(
            ai_tool_policy_groups(&settings)[1].state(),
            AiToolPolicyGroupState::NoneApproved
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

    #[test]
    fn acp_agent_presets_create_editable_configs_with_unique_ids() {
        let mut settings = PersistedSettings::default();

        ai_add_acp_agent_preset(&mut settings, AcpAgentPreset::ClaudeCode);
        ai_add_acp_agent_preset(&mut settings, AcpAgentPreset::Codex);
        ai_add_acp_agent_preset(&mut settings, AcpAgentPreset::GeminiCli);
        ai_add_acp_agent_preset(&mut settings, AcpAgentPreset::GithubCopilot);
        ai_add_acp_agent_preset(&mut settings, AcpAgentPreset::OpenCode);
        ai_add_acp_agent_preset(&mut settings, AcpAgentPreset::Codex);

        let claude = &settings.ai.acp_agents[0];
        assert_eq!(claude.id, "claude-code");
        assert_eq!(claude.command, "oxideterm-native");
        assert_eq!(
            claude.args,
            vec!["--acp-adapter".to_string(), "claude-code".to_string()]
        );
        assert!(!claude.capability_policy.terminal);

        let codex = &settings.ai.acp_agents[1];
        assert_eq!(codex.id, "codex");
        assert_eq!(codex.command, "oxideterm-native");
        assert_eq!(
            codex.args,
            vec!["--acp-adapter".to_string(), "codex".to_string()]
        );

        let gemini = &settings.ai.acp_agents[2];
        assert_eq!(gemini.id, "gemini-cli");
        assert_eq!(gemini.display_name, "Gemini CLI");
        assert_eq!(gemini.command, "gemini");
        assert_eq!(gemini.args, vec!["--acp".to_string()]);
        assert!(gemini.env.is_empty());

        let copilot = &settings.ai.acp_agents[3];
        assert_eq!(copilot.id, "github-copilot");
        assert_eq!(copilot.command, "copilot");
        assert_eq!(
            copilot.args,
            vec!["--acp".to_string(), "--stdio".to_string()]
        );

        let opencode = &settings.ai.acp_agents[4];
        assert_eq!(opencode.id, "opencode");
        assert_eq!(opencode.display_name, "OpenCode");
        assert_eq!(opencode.command, "opencode");
        assert_eq!(opencode.args, vec!["acp".to_string()]);
        assert!(opencode.env.is_empty());

        assert_eq!(settings.ai.acp_agents[5].id, "codex-2");
    }
}

pub const AI_MODEL_REFRESH_MISSING_API_KEY: &str = "__missing_api_key__";

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

impl Zeroize for AiMcpServerDraft {
    fn zeroize(&mut self) {
        // Arguments, environment values, headers, and tokens may all contain
        // credentials supplied by the user.
        self.name.zeroize();
        self.command.zeroize();
        self.args.zeroize();
        for (key, value) in &mut self.env {
            key.zeroize();
            value.zeroize();
        }
        self.env.clear();
        self.url.zeroize();
        self.auth_header_name.zeroize();
        self.auth_token.zeroize();
        for (key, value) in &mut self.headers {
            key.zeroize();
            value.zeroize();
        }
        self.headers.clear();
        self.retry_on_disconnect = false;
        self.show_auth_token = false;
    }
}

impl ZeroizeOnDrop for AiMcpServerDraft {}

impl Drop for AiMcpServerDraft {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl fmt::Debug for AiMcpServerDraft {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // MCP drafts may hold auth tokens plus env/header/argument values
        // commonly used for API keys. Keep Debug structural so diagnostics
        // cannot leak them.
        formatter
            .debug_struct("AiMcpServerDraft")
            .field("name", &redacted_if_present(&self.name))
            .field("transport", &self.transport)
            .field("command", &redacted_if_present(&self.command))
            .field("args", &redacted_if_present(&self.args))
            .field("env_entry_count", &self.env.len())
            .field("url", &redacted_if_present(&self.url))
            .field(
                "auth_header_name",
                &redacted_if_present(&self.auth_header_name),
            )
            .field("auth_header_mode", &self.auth_header_mode)
            .field("auth_token", &redacted_if_present(&self.auth_token))
            .field("header_entry_count", &self.headers.len())
            .field("retry_on_disconnect", &self.retry_on_disconnect)
            .field("show_auth_token", &self.show_auth_token)
            .finish()
    }
}

fn redacted_if_present(value: &str) -> Option<&'static str> {
    (!value.is_empty()).then_some("<redacted>")
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
    let configured_names = ai_mcp_configs(settings)
        .into_iter()
        .map(|server| server.name)
        .collect();
    ai_mcp_draft_valid_for_names(draft, &configured_names)
}

pub fn ai_mcp_draft_valid_for_names(
    draft: &AiMcpServerDraft,
    configured_names: &HashSet<String>,
) -> bool {
    let name = draft.name.trim();
    !name.is_empty()
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
        && !configured_names.contains(name)
}

pub fn ai_mcp_draft_input_value(
    draft: Option<&AiMcpServerDraft>,
    input: SettingsInput,
) -> Option<&str> {
    let draft = draft?;
    match input {
        SettingsInput::AiMcpName => Some(&draft.name),
        SettingsInput::AiMcpCommand => Some(&draft.command),
        SettingsInput::AiMcpArgs => Some(&draft.args),
        SettingsInput::AiMcpUrl => Some(&draft.url),
        SettingsInput::AiMcpAuthHeaderName => Some(&draft.auth_header_name),
        SettingsInput::AiMcpAuthToken => Some(&draft.auth_token),
        SettingsInput::AiMcpEnvKey(index) => draft.env.get(index).map(|(key, _)| key.as_str()),
        SettingsInput::AiMcpEnvValue(index) => {
            draft.env.get(index).map(|(_, value)| value.as_str())
        }
        SettingsInput::AiMcpHeaderKey(index) => {
            draft.headers.get(index).map(|(key, _)| key.as_str())
        }
        SettingsInput::AiMcpHeaderValue(index) => {
            draft.headers.get(index).map(|(_, value)| value.as_str())
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
