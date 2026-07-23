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
    #[serde(default)]
    pub transfer_protocol: FileTransferProtocolPreference,
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
            transfer_protocol: FileTransferProtocolPreference::Auto,
            max_concurrent_transfers: 3,
            directory_parallelism: 4,
            speed_limit_enabled: false,
            speed_limit_kbps: 0,
            conflict_action: ConflictAction::Ask,
            extra: ExtraFields::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileTransferProtocolPreference {
    #[default]
    Auto,
    Sftp,
    Scp,
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

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingsUpstreamProxyProtocol {
    Socks5,
    HttpConnect,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SettingsUpstreamProxyAuth {
    None,
    Password {
        username: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        keychain_id: Option<String>,
    },
}

impl Default for SettingsUpstreamProxyAuth {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsUpstreamProxyConfig {
    pub protocol: SettingsUpstreamProxyProtocol,
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub auth: SettingsUpstreamProxyAuth,
    #[serde(default = "default_proxy_remote_dns")]
    pub remote_dns: bool,
    #[serde(default)]
    pub no_proxy: String,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SettingsApplicationProxyMode {
    #[default]
    System,
    Direct,
    Shared,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_proxy: Option<SettingsUpstreamProxyConfig>,
    #[serde(default)]
    pub upstream_proxy_disclaimer_accepted: bool,
    #[serde(default)]
    pub application_proxy_mode: SettingsApplicationProxyMode,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NetworkSettingsCompat {
    #[serde(default)]
    upstream_proxy: Option<SettingsUpstreamProxyConfig>,
    #[serde(default)]
    upstream_proxy_disclaimer_accepted: bool,
    #[serde(default)]
    application_proxy_mode: Option<SettingsApplicationProxyMode>,
    #[serde(default)]
    application_proxy_enabled: Option<bool>,
    #[serde(flatten)]
    extra: ExtraFields,
}

impl<'de> Deserialize<'de> for NetworkSettings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let legacy = NetworkSettingsCompat::deserialize(deserializer)?;
        // Older settings stored only whether the shared proxy handled app traffic.
        let application_proxy_mode = legacy.application_proxy_mode.unwrap_or_else(|| {
            if legacy.application_proxy_enabled.unwrap_or(false) {
                SettingsApplicationProxyMode::Shared
            } else {
                SettingsApplicationProxyMode::System
            }
        });
        Ok(Self {
            upstream_proxy: legacy.upstream_proxy,
            upstream_proxy_disclaimer_accepted: legacy.upstream_proxy_disclaimer_accepted,
            application_proxy_mode,
            extra: legacy.extra,
        })
    }
}

fn default_proxy_remote_dns() -> bool {
    true
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
pub struct SshConfigSettings {
    pub auto_load_hosts: bool,
    #[serde(default)]
    pub auto_sync_hosts: bool,
    #[serde(default)]
    pub allow_proxy_command: bool,
}

impl Default for SshConfigSettings {
    fn default() -> Self {
        Self {
            auto_load_hosts: true,
            auto_sync_hosts: false,
            allow_proxy_command: false,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsSettings {
    pub debug_logging: bool,
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
    #[serde(default)]
    pub settings_navigation: SettingsNavigationSettings,
    pub ai: AiSettings,
    pub local_terminal: LocalTerminalSettings,
    pub sftp: SftpSettings,
    pub ide: IdeSettings,
    pub reconnect: ReconnectSettings,
    pub connection_pool: ConnectionPoolSettings,
    #[serde(default)]
    pub network: NetworkSettings,
    pub experimental: ExperimentalSettings,
    // Legal acceptance is independent from whether the optional welcome flow is complete.
    #[serde(default)]
    pub onboarding_disclaimer_accepted: bool,
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
    #[serde(default)]
    pub ssh_config: SshConfigSettings,
    #[serde(default)]
    pub diagnostics: DiagnosticsSettings,
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
            settings_navigation: SettingsNavigationSettings::default(),
            ai: AiSettings::default(),
            local_terminal: LocalTerminalSettings::default(),
            sftp: SftpSettings::default(),
            ide: IdeSettings::default(),
            reconnect: ReconnectSettings::default(),
            connection_pool: ConnectionPoolSettings::default(),
            network: NetworkSettings::default(),
            experimental: ExperimentalSettings::default(),
            onboarding_disclaimer_accepted: false,
            onboarding_completed: false,
            command_palette_mru: Vec::new(),
            keybindings: KeybindingSettings::default(),
            custom_themes: Map::new(),
            launcher: LauncherSettings::default(),
            agent_roles: None,
            new_connection: NewConnectionSettings::default(),
            ssh_config: SshConfigSettings::default(),
            diagnostics: DiagnosticsSettings::default(),
            extra: ExtraFields::new(),
        }
    }
}

impl PersistedSettings {
    pub fn record_command_palette_use(&mut self, command_id: &str) {
        const MAX_COMMAND_PALETTE_MRU_ENTRIES: usize = 20;

        self.command_palette_mru
            .retain(|candidate| candidate != command_id);
        self.command_palette_mru.insert(0, command_id.to_string());
        self.command_palette_mru
            .truncate(MAX_COMMAND_PALETTE_MRU_ENTRIES);
    }

    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).expect("settings should serialize")
    }
}

#[cfg(test)]
mod misc_tests {
    use super::{PersistedSettings, SettingsApplicationProxyMode};
    use crate::DEFAULT_WINDOW_OPACITY;

    #[test]
    fn command_palette_mru_deduplicates_promotes_and_bounds_entries() {
        let mut settings = PersistedSettings::default();
        settings.command_palette_mru = (0..20).map(|index| format!("command-{index}")).collect();

        settings.record_command_palette_use("command-10");
        assert_eq!(settings.command_palette_mru[0], "command-10");
        assert_eq!(settings.command_palette_mru.len(), 20);
        assert_eq!(
            settings
                .command_palette_mru
                .iter()
                .filter(|command| command.as_str() == "command-10")
                .count(),
            1
        );

        settings.record_command_palette_use("new-command");
        assert_eq!(settings.command_palette_mru[0], "new-command");
        assert_eq!(settings.command_palette_mru.len(), 20);
    }

    #[test]
    fn settings_navigation_groups_round_trip_with_camel_case_key() {
        let mut settings = PersistedSettings::default();
        settings.settings_navigation.groups = vec![
            vec!["terminal".to_string(), "general".to_string()],
            vec!["appearance".to_string()],
        ];

        let serialized = settings.to_value();
        let restored: PersistedSettings =
            serde_json::from_value(serialized.clone()).expect("settings should deserialize");

        assert_eq!(
            serialized["settingsNavigation"]["groups"][0][0],
            "terminal"
        );
        assert_eq!(restored.settings_navigation, settings.settings_navigation);
    }

    #[test]
    fn onboarding_disclaimer_acceptance_round_trips_independently() {
        let mut settings = PersistedSettings::default();
        settings.onboarding_disclaimer_accepted = true;

        let serialized = settings.to_value();
        let restored: PersistedSettings =
            serde_json::from_value(serialized.clone()).expect("settings should deserialize");

        assert_eq!(serialized["onboardingDisclaimerAccepted"], true);
        assert!(restored.onboarding_disclaimer_accepted);
        assert!(!restored.onboarding_completed);
    }

    #[test]
    fn legacy_settings_default_onboarding_disclaimer_acceptance_when_missing() {
        let mut serialized = PersistedSettings::default().to_value();
        serialized
            .as_object_mut()
            .expect("settings should be an object")
            .remove("onboardingDisclaimerAccepted");

        let restored: PersistedSettings =
            serde_json::from_value(serialized).expect("legacy settings should deserialize");

        assert!(!restored.onboarding_disclaimer_accepted);
    }

    #[test]
    fn legacy_appearance_settings_default_to_visible_window_titlebar() {
        let mut serialized = PersistedSettings::default().to_value();
        serialized["appearance"]
            .as_object_mut()
            .expect("appearance should be an object")
            .remove("showWindowTitlebar");

        let restored: PersistedSettings =
            serde_json::from_value(serialized).expect("legacy settings should deserialize");

        assert!(restored.appearance.show_window_titlebar);
        assert_eq!(restored.appearance.window_opacity, DEFAULT_WINDOW_OPACITY);
    }

    #[test]
    fn legacy_settings_default_to_automatic_ssh_config_discovery() {
        let mut serialized = PersistedSettings::default().to_value();
        serialized
            .as_object_mut()
            .expect("settings should be an object")
            .remove("sshConfig");

        let restored: PersistedSettings =
            serde_json::from_value(serialized).expect("legacy settings should deserialize");

        assert!(restored.ssh_config.auto_load_hosts);
        assert!(!restored.ssh_config.auto_sync_hosts);
        assert!(!restored.ssh_config.allow_proxy_command);
    }

    #[test]
    fn legacy_application_proxy_flag_migrates_to_explicit_routing_mode() {
        let mut serialized = PersistedSettings::default().to_value();
        let network = serialized["network"]
            .as_object_mut()
            .expect("network settings should be an object");
        network.remove("applicationProxyMode");
        network.insert("applicationProxyEnabled".to_string(), true.into());

        let restored: PersistedSettings =
            serde_json::from_value(serialized).expect("legacy settings should deserialize");

        assert_eq!(
            restored.network.application_proxy_mode,
            SettingsApplicationProxyMode::Shared
        );
    }

    #[test]
    fn application_proxy_mode_serializes_without_the_legacy_flag() {
        let mut settings = PersistedSettings::default();
        settings.network.application_proxy_mode = SettingsApplicationProxyMode::Direct;

        let serialized = settings.to_value();

        assert_eq!(serialized["network"]["applicationProxyMode"], "direct");
        assert!(serialized["network"].get("applicationProxyEnabled").is_none());
    }
}
