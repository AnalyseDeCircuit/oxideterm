#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiMemorySettings {
    pub enabled: bool,
    pub content: String,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for AiMemorySettings {
    fn default() -> Self {
        Self {
            enabled: true,
            content: String::new(),
            extra: ExtraFields::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiToolUseSettings {
    pub enabled: bool,
    pub auto_approve_tools: Map<String, Value>,
    pub disabled_tools: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_rounds: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_calls_per_round: Option<i64>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for AiToolUseSettings {
    fn default() -> Self {
        let mut auto_approve_tools = Map::new();
        for (name, enabled) in [
            ("list_targets", true),
            ("select_target", true),
            ("observe_terminal", true),
            ("read_resource", true),
            ("get_state", true),
            ("recall_preferences", true),
            ("connect_target", false),
            ("run_command", false),
            ("send_terminal_input", false),
            ("write_resource", false),
            ("write_resource:settings", false),
            ("write_resource:file", false),
            ("transfer_resource", false),
            ("open_app_surface", false),
            ("remember_preference", false),
        ] {
            auto_approve_tools.insert(name.to_string(), json!(enabled));
        }
        Self {
            enabled: false,
            auto_approve_tools,
            disabled_tools: Vec::new(),
            max_rounds: Some(DEFAULT_AI_TOOL_MAX_ROUNDS),
            max_calls_per_round: Some(DEFAULT_AI_TOOL_MAX_CALLS_PER_ROUND),
            extra: ExtraFields::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiContextSources {
    pub ide: bool,
    pub sftp: bool,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for AiContextSources {
    fn default() -> Self {
        Self {
            ide: true,
            sftp: true,
            extra: ExtraFields::new(),
        }
    }
}

fn default_execution_profiles() -> Value {
    json!({
        "defaultProfileId": "default",
        "profiles": [{
            "id": "default",
            "name": "Default",
            "backend": "provider",
            "providerId": null,
            "acpAgentId": null,
            "model": null,
            "reasoningEffort": "auto",
            "toolUse": {
                "enabled": false,
                "maxRounds": DEFAULT_AI_TOOL_MAX_ROUNDS,
                "maxCallsPerRound": DEFAULT_AI_TOOL_MAX_CALLS_PER_ROUND,
                "autoApproveTools": {},
                "disabledTools": []
            },
            "context": {
                "includeRuntimeChips": true,
                "includeMemory": true,
                "includeRag": true
            },
            "commandPolicy": { "allow": [], "deny": [] },
            "createdAt": 0,
            "updatedAt": 0
        }]
    })
}

fn default_acp_agent_enabled() -> bool {
    true
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AcpAgentAuthStatus {
    #[default]
    Unknown,
    NotRequired,
    Required,
    Authenticated,
    Expired,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpAgentAuthState {
    #[serde(default)]
    pub status: AcpAgentAuthStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_label: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpAgentCapabilityPolicy {
    #[serde(default)]
    pub fs_read_text_file: bool,
    #[serde(default)]
    pub fs_write_text_file: bool,
    #[serde(default)]
    pub terminal: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AcpAgentRuntimeState {
    #[default]
    Unknown,
    Ready,
    AuthRequired,
    Error,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpAgentRuntimeStatus {
    #[serde(default)]
    pub state: AcpAgentRuntimeState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error_kind: Option<String>,
}

#[derive(Clone, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpAgentConfig {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default = "default_acp_agent_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub auth: AcpAgentAuthState,
    #[serde(default)]
    pub capability_policy: AcpAgentCapabilityPolicy,
    #[serde(default)]
    pub status: AcpAgentRuntimeStatus,
}

impl std::fmt::Debug for AcpAgentConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AcpAgentConfig")
            .field("id", &self.id)
            .field("display_name", &self.display_name)
            .field("command", &self.command)
            // Args and env values can contain tokens, so Debug only exposes shape.
            .field("args", &format_args!("<redacted:{}>", self.args.len()))
            .field("env", &format_args!("<redacted:{}>", self.env.len()))
            .field("cwd", &self.cwd)
            .field("enabled", &self.enabled)
            .field("auth", &self.auth)
            .field("capability_policy", &self.capability_policy)
            .field("status", &self.status)
            .finish()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSettings {
    pub enabled: bool,
    pub enabled_confirmed: bool,
    pub base_url: String,
    pub model: String,
    pub providers: Vec<Value>,
    pub active_provider_id: Option<String>,
    pub active_model: Option<String>,
    pub context_max_chars: i64,
    pub context_visible_lines: i64,
    pub thinking_style: AiThinkingStyle,
    pub reasoning_effort: AiReasoningEffort,
    pub reasoning_provider_overrides: Map<String, Value>,
    pub reasoning_model_overrides: Map<String, Value>,
    pub thinking_default_expanded: bool,
    #[serde(default)]
    pub model_context_windows: Map<String, Value>,
    #[serde(default)]
    pub user_context_windows: Map<String, Value>,
    pub custom_system_prompt: String,
    pub memory: AiMemorySettings,
    #[serde(default)]
    pub model_max_response_tokens: Map<String, Value>,
    pub tool_use: AiToolUseSettings,
    pub context_sources: AiContextSources,
    #[serde(default)]
    pub mcp_servers: Vec<Value>,
    #[serde(default)]
    pub acp_agents: Vec<AcpAgentConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_config: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_roles: Option<Value>,
    pub execution_profiles: Value,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            enabled_confirmed: false,
            base_url: "https://api.openai.com/v1".to_string(),
            model: "gpt-4o-mini".to_string(),
            providers: Vec::new(),
            active_provider_id: None,
            active_model: None,
            context_max_chars: 8000,
            context_visible_lines: 120,
            thinking_style: AiThinkingStyle::Detailed,
            reasoning_effort: AiReasoningEffort::Auto,
            reasoning_provider_overrides: Map::new(),
            reasoning_model_overrides: Map::new(),
            thinking_default_expanded: false,
            model_context_windows: Map::new(),
            user_context_windows: Map::new(),
            custom_system_prompt: String::new(),
            memory: AiMemorySettings::default(),
            model_max_response_tokens: Map::new(),
            tool_use: AiToolUseSettings::default(),
            context_sources: AiContextSources::default(),
            mcp_servers: Vec::new(),
            acp_agents: Vec::new(),
            embedding_config: None,
            agent_roles: None,
            execution_profiles: default_execution_profiles(),
            extra: ExtraFields::new(),
        }
    }
}

#[cfg(test)]
mod ai_model_tests {
    use super::*;

    #[test]
    fn acp_agent_defaults_keep_host_capabilities_closed() {
        let agent: AcpAgentConfig = serde_json::from_value(json!({
            "id": "codex-local",
            "displayName": "Codex Local",
            "command": "codex"
        }))
        .expect("agent config");

        assert!(agent.enabled);
        assert!(!agent.capability_policy.fs_read_text_file);
        assert!(!agent.capability_policy.fs_write_text_file);
        assert!(!agent.capability_policy.terminal);
    }

    #[test]
    fn acp_agent_debug_redacts_args_and_env_values() {
        let agent: AcpAgentConfig = serde_json::from_value(json!({
            "id": "codex-local",
            "displayName": "Codex Local",
            "command": "codex",
            "args": ["--api-key=arg-secret"],
            "env": { "API_KEY": "env-secret" },
            "auth": { "status": "authenticated", "accountLabel": "user@example.test" }
        }))
        .expect("agent config");

        let debug = format!("{agent:?}");

        assert!(debug.contains("<redacted:1>"));
        assert!(!debug.contains("arg-secret"));
        assert!(!debug.contains("env-secret"));
    }

    #[test]
    fn acp_agent_serialization_drops_unknown_secret_fields() {
        let agent: AcpAgentConfig = serde_json::from_value(json!({
            "id": "codex-local",
            "displayName": "Codex Local",
            "command": "codex",
            "authToken": "legacy-secret",
            "auth": {
                "status": "authenticated",
                "accountLabel": "user@example.test",
                "token": "auth-secret"
            },
            "status": {
                "state": "ready",
                "lastErrorKind": "none",
                "stderr": "stderr-secret"
            }
        }))
        .expect("agent config");

        let serialized = serde_json::to_string(&agent).expect("agent json");

        assert!(serialized.contains("user@example.test"));
        assert!(!serialized.contains("legacy-secret"));
        assert!(!serialized.contains("auth-secret"));
        assert!(!serialized.contains("stderr-secret"));
    }
}
