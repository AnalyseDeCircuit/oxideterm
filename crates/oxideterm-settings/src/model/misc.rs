#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalTerminalSettings {
    pub default_shell_id: Option<String>,
    pub recent_shell_ids: Vec<String>,
    pub default_cwd: Option<String>,
    pub git_bash_path: Option<String>,
    pub load_shell_profile: bool,
    pub oh_my_posh_enabled: bool,
    pub oh_my_posh_theme: Option<String>,
    pub custom_env_vars: Map<String, Value>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for LocalTerminalSettings {
    fn default() -> Self {
        Self {
            default_shell_id: None,
            recent_shell_ids: Vec::new(),
            default_cwd: None,
            git_bash_path: None,
            load_shell_profile: true,
            oh_my_posh_enabled: false,
            oh_my_posh_theme: None,
            custom_env_vars: Map::new(),
            extra: ExtraFields::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpSettings {
    pub max_concurrent_transfers: i64,
    pub directory_parallelism: i64,
    pub speed_limit_enabled: bool,
    #[serde(rename = "speedLimitKBps", alias = "speedLimitKbps")]
    pub speed_limit_kbps: i64,
    pub conflict_action: ConflictAction,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for SftpSettings {
    fn default() -> Self {
        Self {
            max_concurrent_transfers: 3,
            directory_parallelism: 4,
            speed_limit_enabled: false,
            speed_limit_kbps: 0,
            conflict_action: ConflictAction::Ask,
            extra: ExtraFields::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IdeSettings {
    pub auto_save: bool,
    pub font_size: Option<i64>,
    pub line_height: Option<f64>,
    pub agent_mode: IdeAgentMode,
    pub word_wrap: bool,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for IdeSettings {
    fn default() -> Self {
        Self {
            auto_save: false,
            font_size: None,
            line_height: None,
            agent_mode: IdeAgentMode::Ask,
            word_wrap: false,
            extra: ExtraFields::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconnectSettings {
    pub enabled: bool,
    pub max_attempts: i64,
    pub base_delay_ms: i64,
    pub max_delay_ms: i64,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for ReconnectSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            max_attempts: 5,
            base_delay_ms: 1000,
            max_delay_ms: 15_000,
            extra: ExtraFields::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionPoolSettings {
    pub idle_timeout_secs: i64,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for ConnectionPoolSettings {
    fn default() -> Self {
        Self {
            idle_timeout_secs: 1800,
            extra: ExtraFields::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentalSettings {
    pub virtual_session_proxy: bool,
    pub gpu_canvas: bool,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for ExperimentalSettings {
    fn default() -> Self {
        Self {
            virtual_session_proxy: false,
            gpu_canvas: false,
            extra: ExtraFields::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeybindingSettings {
    pub overrides: Map<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherSettings {
    pub enabled: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewConnectionSettings {
    pub save_connection: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedSettings {
    pub version: u32,
    pub general: GeneralSettings,
    pub terminal: TerminalSettings,
    pub buffer: BufferSettings,
    pub appearance: AppearanceSettings,
    pub connection_defaults: ConnectionDefaults,
    #[serde(rename = "treeUI")]
    pub tree_ui: TreeUiState,
    #[serde(rename = "sidebarUI")]
    pub sidebar_ui: SidebarUiState,
    pub ai: AiSettings,
    pub local_terminal: LocalTerminalSettings,
    pub sftp: SftpSettings,
    pub ide: IdeSettings,
    pub reconnect: ReconnectSettings,
    pub connection_pool: ConnectionPoolSettings,
    pub experimental: ExperimentalSettings,
    pub onboarding_completed: bool,
    #[serde(default)]
    pub command_palette_mru: Vec<String>,
    #[serde(default)]
    pub keybindings: KeybindingSettings,
    #[serde(default)]
    pub custom_themes: Map<String, Value>,
    #[serde(default)]
    pub launcher: LauncherSettings,
    #[serde(default)]
    pub agent_roles: Option<Value>,
    #[serde(default)]
    pub new_connection: NewConnectionSettings,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for PersistedSettings {
    fn default() -> Self {
        Self {
            version: SETTINGS_SCHEMA_VERSION,
            general: GeneralSettings::default(),
            terminal: TerminalSettings::default(),
            buffer: BufferSettings::default(),
            appearance: AppearanceSettings::default(),
            connection_defaults: ConnectionDefaults::default(),
            tree_ui: TreeUiState::default(),
            sidebar_ui: SidebarUiState::default(),
            ai: AiSettings::default(),
            local_terminal: LocalTerminalSettings::default(),
            sftp: SftpSettings::default(),
            ide: IdeSettings::default(),
            reconnect: ReconnectSettings::default(),
            connection_pool: ConnectionPoolSettings::default(),
            experimental: ExperimentalSettings::default(),
            onboarding_completed: false,
            command_palette_mru: Vec::new(),
            keybindings: KeybindingSettings::default(),
            custom_themes: Map::new(),
            launcher: LauncherSettings::default(),
            agent_roles: None,
            new_connection: NewConnectionSettings::default(),
            extra: ExtraFields::new(),
        }
    }
}

impl PersistedSettings {
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).expect("settings should serialize")
    }
}
