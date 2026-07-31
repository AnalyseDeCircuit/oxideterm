// Shared policy keys keep persisted settings, runtime policy, and settings UI aligned.
pub const AI_TOOL_CREATE_BACKGROUND_TASK: &str = "create_background_task";
pub const AI_TOOL_LIST_BACKGROUND_TASKS: &str = "list_background_tasks";
pub const AI_TOOL_GET_BACKGROUND_TASK: &str = "get_background_task";
pub const AI_TOOL_CANCEL_BACKGROUND_TASK: &str = "cancel_background_task";
pub const AI_TOOL_INSPECT_HOST_TOOLS: &str = "inspect_host_tools";
pub const AI_TOOL_CONTROL_HOST_TOOL: &str = "control_host_tool";
pub const AI_TOOL_LIST_FORWARDS: &str = "list_forwards";
pub const AI_TOOL_MANAGE_FORWARD: &str = "manage_forward";
pub const AI_TOOL_LIST_PLUGINS: &str = "list_plugins";
pub const AI_TOOL_MANAGE_PLUGIN: &str = "manage_plugin";
pub const AI_TOOL_LIST_TRANSPORT_PROFILES: &str = "list_transport_profiles";
pub const AI_TOOL_OPEN_TRANSPORT_PROFILE: &str = "open_transport_profile";
pub const AI_TOOL_GET_TRANSPORT_SESSION_STATE: &str = "get_transport_session_state";
pub const AI_TOOL_MANAGE_SERIAL_SESSION: &str = "manage_serial_session";
pub const AI_TOOL_MANAGE_TELNET_SESSION: &str = "manage_telnet_session";
pub const AI_TOOL_LIST_REMOTE_DESKTOP_SESSIONS: &str = "list_remote_desktop_sessions";
pub const AI_TOOL_MANAGE_REMOTE_DESKTOP_SESSION: &str = "manage_remote_desktop_session";
pub const AI_TOOL_GET_CLOUD_SYNC_STATE: &str = "get_cloud_sync_state";
pub const AI_TOOL_MANAGE_CLOUD_SYNC: &str = "manage_cloud_sync";
pub const AI_TOOL_LIST_CREDENTIALS: &str = "list_credentials";
pub const AI_TOOL_MANAGE_CREDENTIAL: &str = "manage_credential";
pub const AI_TOOL_LOAD_SKILL: &str = "load_skill";
pub const AI_TOOL_READ_SKILL_RESOURCE: &str = "read_skill_resource";
pub const AI_APPLICATION_WORKSPACE_MEMORY_SCOPE_ID: &str = "application-workspace";

fn default_ai_skills_enabled() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSkillsSettings {
    #[serde(default = "default_ai_skills_enabled")]
    pub enabled: bool,
    /// Canonical SKILL.md files or skill roots disabled by the user.
    #[serde(default)]
    pub disabled_paths: Vec<String>,
}

impl Default for AiSkillsSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            disabled_paths: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AiMemoryScopeKind {
    #[default]
    User,
    Workspace,
    Project,
    Host,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AiMemoryKind {
    #[default]
    LongTerm,
    Temporary,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AiMemorySource {
    Manual,
    #[default]
    Assistant,
    Migrated,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiMemoryEntry {
    pub id: String,
    pub content: String,
    #[serde(default)]
    pub scope_kind: AiMemoryScopeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_id: Option<String>,
    #[serde(default)]
    pub memory_kind: AiMemoryKind,
    #[serde(default)]
    pub source: AiMemorySource,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_at_ms: Option<i64>,
    #[serde(default)]
    pub use_count: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_ms: Option<i64>,
    #[serde(default)]
    pub revision: u64,
}

impl AiMemoryEntry {
    pub fn is_expired(&self, now_ms: i64) -> bool {
        self.expires_at_ms
            .is_some_and(|expires_at_ms| expires_at_ms <= now_ms)
    }

    pub fn applies_to(
        &self,
        user_id: Option<&str>,
        workspace_id: Option<&str>,
        project_id: Option<&str>,
        host_id: Option<&str>,
    ) -> bool {
        let expected_scope = match self.scope_kind {
            AiMemoryScopeKind::User => user_id,
            AiMemoryScopeKind::Workspace => workspace_id,
            AiMemoryScopeKind::Project => project_id,
            AiMemoryScopeKind::Host => host_id,
        };
        match self.scope_id.as_deref() {
            Some(scope_id) => expected_scope == Some(scope_id),
            None => self.scope_kind == AiMemoryScopeKind::User,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiMemorySettings {
    pub enabled: bool,
    /// Legacy free-form memory is retained for backward-compatible settings files.
    pub content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<AiMemoryEntry>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for AiMemorySettings {
    fn default() -> Self {
        Self {
            enabled: true,
            content: String::new(),
            entries: Vec::new(),
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
            ("wait_terminal_output", true),
            ("get_terminal_command_status", true),
            ("read_resource", true),
            ("get_state", true),
            ("recall_preferences", true),
            ("list_memory_entries", true),
            (AI_TOOL_LIST_BACKGROUND_TASKS, true),
            (AI_TOOL_GET_BACKGROUND_TASK, true),
            (AI_TOOL_INSPECT_HOST_TOOLS, true),
            (AI_TOOL_LIST_FORWARDS, true),
            (AI_TOOL_LIST_PLUGINS, true),
            (AI_TOOL_LIST_TRANSPORT_PROFILES, true),
            (AI_TOOL_GET_TRANSPORT_SESSION_STATE, true),
            (AI_TOOL_LIST_REMOTE_DESKTOP_SESSIONS, true),
            (AI_TOOL_GET_CLOUD_SYNC_STATE, true),
            (AI_TOOL_LIST_CREDENTIALS, true),
            (AI_TOOL_LOAD_SKILL, true),
            (AI_TOOL_READ_SKILL_RESOURCE, true),
            ("connect_target", false),
            ("run_command", false),
            ("send_terminal_input", false),
            ("write_resource", false),
            ("write_resource:settings", false),
            ("write_resource:file", false),
            ("transfer_resource", false),
            ("open_app_surface", false),
            ("remember_preference", false),
            ("manage_memory_entry", false),
            (AI_TOOL_CREATE_BACKGROUND_TASK, false),
            (AI_TOOL_CANCEL_BACKGROUND_TASK, false),
            (AI_TOOL_CONTROL_HOST_TOOL, false),
            (AI_TOOL_MANAGE_FORWARD, false),
            (AI_TOOL_MANAGE_PLUGIN, false),
            (AI_TOOL_OPEN_TRANSPORT_PROFILE, false),
            (AI_TOOL_MANAGE_SERIAL_SESSION, false),
            (AI_TOOL_MANAGE_TELNET_SESSION, false),
            (AI_TOOL_MANAGE_REMOTE_DESKTOP_SESSION, false),
            (AI_TOOL_MANAGE_CLOUD_SYNC, false),
            (AI_TOOL_MANAGE_CREDENTIAL, false),
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

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AiActiveBackend {
    #[default]
    Provider,
    Acp,
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
    #[serde(default)]
    pub active_backend: AiActiveBackend,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_acp_agent_id: Option<String>,
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
    pub skills: AiSkillsSettings,
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
            active_backend: AiActiveBackend::Provider,
            active_acp_agent_id: None,
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
            skills: AiSkillsSettings::default(),
            model_max_response_tokens: Map::new(),
            tool_use: AiToolUseSettings::default(),
            context_sources: AiContextSources::default(),
            mcp_servers: Vec::new(),
            acp_agents: Vec::new(),
            embedding_config: None,
            agent_roles: None,
            extra: ExtraFields::new(),
        }
    }
}

#[cfg(test)]
mod ai_model_tests {
    use super::*;

    fn memory_entry(scope_kind: AiMemoryScopeKind, scope_id: Option<&str>) -> AiMemoryEntry {
        AiMemoryEntry {
            id: "memory-1".to_string(),
            content: "Use fish on this host.".to_string(),
            scope_kind,
            scope_id: scope_id.map(str::to_string),
            memory_kind: AiMemoryKind::LongTerm,
            source: AiMemorySource::Manual,
            created_at_ms: 10,
            updated_at_ms: 10,
            last_used_at_ms: None,
            use_count: 0,
            expires_at_ms: None,
            revision: 1,
        }
    }

    #[test]
    fn memory_scope_matches_only_its_runtime_identity() {
        let host_memory = memory_entry(AiMemoryScopeKind::Host, Some("host-a"));

        assert!(host_memory.applies_to(
            Some("user-a"),
            Some("workspace-a"),
            Some("/project-a"),
            Some("host-a"),
        ));
        assert!(!host_memory.applies_to(
            Some("user-a"),
            Some("workspace-a"),
            Some("/project-a"),
            Some("host-b"),
        ));
    }

    #[test]
    fn temporary_memory_expires_at_its_deadline() {
        let mut entry = memory_entry(AiMemoryScopeKind::User, None);
        entry.memory_kind = AiMemoryKind::Temporary;
        entry.expires_at_ms = Some(100);

        assert!(!entry.is_expired(99));
        assert!(entry.is_expired(100));
    }

    #[test]
    fn skills_default_to_enabled_for_existing_settings_files() {
        let skills: AiSkillsSettings =
            serde_json::from_value(json!({})).expect("skills settings");

        assert!(skills.enabled);
        assert!(skills.disabled_paths.is_empty());
    }

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
