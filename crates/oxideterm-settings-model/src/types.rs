// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Pure settings page identity types.
//!
//! These enums describe settings navigation, editable fields, selects, and
//! sliders without depending on GPUI. View crates can map them to anchors and
//! controls, while app code can use the same model keys for focus and drafts.

const PLUGIN_MANAGER_INPUT_ANCHOR_BASE: u64 = 28_000;
const PLUGIN_SETTING_INPUT_ANCHOR_BASE: u64 = 29_000;
const DEFAULT_SETTINGS_TEXTAREA_LINE_HEIGHT: f32 = 20.0;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SettingsTab {
    General,
    Portable,
    Terminal,
    Appearance,
    Connections,
    Privilege,
    Network,
    Sftp,
    Ide,
    Ai,
    Knowledge,
    Keybindings,
    Help,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalSettingsPage {
    Display,
    Input,
    Local,
    CommandBar,
    Awareness,
    Transfer,
    Highlight,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiSettingsPage {
    General,
    Providers,
    Agents,
    Context,
    Tools,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsKeybindingScopeFilter {
    All,
    Global,
    Terminal,
    Split,
    Palette,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsSelect {
    Language,
    UpdateChannel,
    UpdateProxyMode,
    UpdateProxyProtocol,
    AppearanceTheme,
    AppearanceDensity,
    AppearanceAnimation,
    AppearanceRenderProfile,
    AppearanceFrostedGlass,
    AppearanceBackgroundFit,
    CustomThemeDuplicate,
    TerminalFontFamily,
    TerminalCjkFontFamily,
    TerminalEncoding,
    TerminalBackspaceSequence,
    TerminalDeleteSequence,
    TerminalCursorStyle,
    RemoteShellIntegrationMode,
    IdeAgentMode,
    LocalShell,
    LocalPrivilegeKind,
    ConnectionIdleTimeout,
    ReconnectMaxAttempts,
    ReconnectBaseDelay,
    ReconnectMaxDelay,
    NetworkApplicationProxyMode,
    NetworkProxyProtocol,
    NetworkProxyAuth,
    AiProviderTemplate,
    AiContextMaxChars,
    AiContextVisibleLines,
    AiGlobalReasoning,
    AiProviderReasoning(usize),
    AiModelReasoning(usize, usize),
    AiEmbeddingProvider,
    KnowledgeCollectionScope,
    KnowledgeDocumentFormat,
    AiMcpTransport,
    AiMcpAuthMode,
    SftpProtocol,
    SftpConcurrent,
    SftpDirectoryParallelism,
    SftpConflict,
    HighlightPreset,
    HighlightRenderMode(usize),
    ConnectionImportSource,
    ConnectionImportDuplicateStrategy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum SettingsInput {
    TerminalCustomFontFamily,
    TerminalFontSize,
    TerminalScrollback,
    TerminalLineHeight,
    IdeFontSize,
    IdeLineHeight,
    AppearanceUiFont,
    LocalDefaultCwd,
    LocalGitBashPath,
    LocalOhMyPoshTheme,
    LocalPrivilegeLabel,
    LocalPrivilegeUsernameHint,
    LocalPrivilegeSecret,
    LocalPrivilegePromptPatterns,
    ConnectionDefaultUsername,
    ConnectionDefaultPort,
    ConnectionImportTargetGroup,
    NetworkProxyHost,
    NetworkProxyPort,
    NetworkProxyNoProxy,
    NetworkProxyUsername,
    NetworkProxyPassword,
    NetworkProxyTestHost,
    NetworkProxyTestPort,
    UpdateProxyHost,
    UpdateProxyPort,
    UpdateProxyNoProxy,
    SftpSpeedLimitKbps,
    InBandTransferMaxChunkBytes,
    InBandTransferMaxFileCount,
    InBandTransferMaxTotalBytes,
    TerminalCommandBarFocusHandoff,
    TerminalCommandSpecsJson,
    KeybindingSearch,
    CustomThemeName,
    CustomThemeTerminalColor(usize),
    CustomThemeUiColor(usize),
    HighlightLabel(usize),
    HighlightPattern(usize),
    HighlightForeground(usize),
    HighlightBackground(usize),
    AiProviderName(usize),
    AiProviderBaseUrl(usize),
    AiProviderDefaultModel(usize),
    AiProviderApiKey(usize),
    AiAcpAgentDisplayName(usize),
    AiAcpAgentCommand(usize),
    AiAcpAgentCwd(usize),
    AiAcpAgentArgs(usize),
    AiAcpAgentEnv(usize),
    AiAcpAgentAuthToken(usize),
    AiSystemPrompt,
    AiMemoryContent,
    AiToolUseMaxRounds,
    AiToolUseMaxCallsPerRound,
    AiModelContextWindow(usize, usize),
    AiActiveModelMaxResponseTokens,
    AiEmbeddingModel,
    AiMcpName,
    AiMcpCommand,
    AiMcpArgs,
    AiMcpUrl,
    AiMcpAuthHeaderName,
    AiMcpAuthToken,
    AiMcpEnvKey(usize),
    AiMcpEnvValue(usize),
    AiMcpHeaderKey(usize),
    AiMcpHeaderValue(usize),
    KnowledgeCollectionName,
    KnowledgeDocumentTitle,
    CloudSyncEndpoint,
    CloudSyncNamespace,
    CloudSyncS3Bucket,
    CloudSyncS3Region,
    CloudSyncGitRepository,
    CloudSyncGitBranch,
    CloudSyncGithubOauthClientId,
    CloudSyncMicrosoftOauthClientId,
    CloudSyncGoogleOauthClientId,
    CloudSyncToken,
    CloudSyncGitToken,
    CloudSyncBasicUsername,
    CloudSyncBasicPassword,
    CloudSyncAccessKeyId,
    CloudSyncSecretAccessKey,
    CloudSyncSessionToken,
    CloudSyncSyncPassword,
    CloudSyncAutoUploadInterval,
    PortableCurrentPassword,
    PortableNewPassword,
    PortableConfirmPassword,
    AppLockCurrentPassword,
    AppLockNewPassword,
    AppLockConfirmPassword,
    NativePluginInstallUrl,
    NativePluginInstallChecksum,
    NativePluginRegistryUrl,
    ManagedKeyFilePath,
    ManagedKeyFileName,
    ManagedKeyFilePassphrase,
    ManagedKeyPasteName,
    ManagedKeyPastePrivateKey,
    ManagedKeyPastePassphrase,
    ManagedKeyRenameName,
    PluginSetting(usize),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsSlider {
    TerminalFontSize,
    AppearanceUiFontSize,
    AppearanceBorderRadius,
    VersionMigrationBorderRadius,
    AppearanceWindowOpacity,
    AppearanceBackgroundOpacity,
    AppearanceBackgroundBlur,
}

impl TerminalSettingsPage {
    pub fn all() -> &'static [Self] {
        &[
            Self::Display,
            Self::Input,
            Self::Local,
            Self::CommandBar,
            Self::Awareness,
            Self::Transfer,
            Self::Highlight,
        ]
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Display => "settings_view.terminal.page_display",
            Self::Input => "settings_view.terminal.page_input",
            Self::Local => "settings_view.terminal.page_local",
            Self::CommandBar => "settings_view.terminal.page_commandBar",
            Self::Awareness => "settings_view.terminal.page_awareness",
            Self::Transfer => "settings_view.terminal.page_transfer",
            Self::Highlight => "settings_view.terminal.page_highlight",
        }
    }
}

impl AiSettingsPage {
    pub fn all() -> &'static [Self] {
        &[
            Self::General,
            Self::Providers,
            Self::Agents,
            Self::Context,
            Self::Tools,
        ]
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::General => "settings_view.ai.page_general",
            Self::Providers => "settings_view.ai.page_providers",
            Self::Agents => "settings_view.ai.page_agents",
            Self::Context => "settings_view.ai.page_context",
            Self::Tools => "settings_view.ai.page_tools",
        }
    }
}

impl SettingsKeybindingScopeFilter {
    pub fn all() -> &'static [Self] {
        &[
            Self::All,
            Self::Global,
            Self::Terminal,
            Self::Split,
            Self::Palette,
        ]
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::All => "settings_view.keybindings.scope_all",
            Self::Global => "settings_view.keybindings.scope_global",
            Self::Terminal => "settings_view.keybindings.scope_terminal",
            Self::Split => "settings_view.keybindings.scope_split",
            Self::Palette => "settings_view.keybindings.scope_palette",
        }
    }
}

impl SettingsTab {
    pub fn all() -> &'static [Self] {
        &[
            Self::General,
            Self::Appearance,
            Self::Keybindings,
            Self::Terminal,
            Self::Portable,
            Self::Connections,
            Self::Network,
            Self::Sftp,
            Self::Privilege,
            Self::Ide,
            Self::Ai,
            Self::Knowledge,
            Self::Help,
        ]
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Portable => "portable",
            Self::Terminal => "terminal",
            Self::Appearance => "appearance",
            Self::Connections => "connections",
            Self::Privilege => "privilege",
            Self::Network => "network",
            Self::Sftp => "sftp",
            Self::Ide => "ide",
            Self::Ai => "ai",
            Self::Knowledge => "knowledge",
            Self::Keybindings => "keybindings",
            Self::Help => "help",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::all().iter().copied().find(|tab| tab.id() == id)
    }

    pub fn groups() -> &'static [&'static [Self]] {
        // Keep navigation groups aligned with user tasks: application preferences,
        // runtime behavior, remote access, productivity, then support.
        &[
            &[Self::General, Self::Appearance, Self::Keybindings],
            &[Self::Terminal, Self::Portable],
            &[
                Self::Connections,
                Self::Network,
                Self::Sftp,
                Self::Privilege,
            ],
            &[Self::Ide, Self::Ai, Self::Knowledge],
            &[Self::Help],
        ]
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::General => "settings.general.title",
            Self::Portable => "settings_view.general.portable_runtime",
            Self::Terminal => "settings.terminal.title",
            Self::Appearance => "settings_view.tabs.appearance",
            Self::Connections => "settings_view.connections.keys_and_connections_title",
            Self::Privilege => "settings_view.tabs.privilege",
            Self::Network => "settings_view.tabs.network",
            Self::Sftp => "settings_view.tabs.sftp",
            Self::Ide => "settings_view.tabs.ide",
            Self::Ai => "settings_view.tabs.ai",
            Self::Knowledge => "settings_view.tabs.knowledge",
            Self::Keybindings => "settings_view.tabs.keybindings",
            Self::Help => "settings_view.tabs.help",
        }
    }

    pub fn title_key(self) -> &'static str {
        match self {
            Self::General => "settings_view.general.title",
            Self::Portable => "settings_view.general.portable_runtime",
            Self::Terminal => "settings_view.terminal.title",
            Self::Appearance => "settings_view.appearance.title",
            Self::Connections => "settings_view.connections.keys_and_connections_title",
            Self::Privilege => "settings_view.privilege_credentials.title",
            Self::Network => "settings_view.network.title",
            Self::Sftp => "settings_view.sftp.title",
            Self::Ide => "settings_view.ide.title",
            Self::Ai => "settings_view.ai.title",
            Self::Knowledge => "settings_view.knowledge.title",
            Self::Keybindings => "settings_view.keybindings.title",
            Self::Help => "settings_view.help.title",
        }
    }

    pub fn description_key(self) -> &'static str {
        match self {
            Self::General => "settings_view.general.description",
            Self::Portable => "settings_view.general.portable_runtime_disabled_hint",
            Self::Terminal => "settings_view.terminal.description",
            Self::Appearance => "settings_view.appearance.description",
            Self::Connections => "settings_view.connections.keys_and_connections_description",
            Self::Privilege => "settings_view.privilege_credentials.description",
            Self::Network => "settings_view.network.description",
            Self::Sftp => "settings_view.sftp.description",
            Self::Ide => "settings_view.ide.description",
            Self::Ai => "settings_view.ai.description",
            Self::Knowledge => "settings_view.knowledge.description",
            Self::Keybindings => "settings_view.keybindings.description",
            Self::Help => "settings_view.help.description",
        }
    }

    pub fn icon(self) -> SettingsTabIcon {
        match self {
            Self::General | Self::Appearance => SettingsTabIcon::Monitor,
            Self::Portable | Self::Sftp => SettingsTabIcon::HardDrive,
            Self::Terminal => SettingsTabIcon::Terminal,
            Self::Connections => SettingsTabIcon::Shield,
            Self::Privilege => SettingsTabIcon::Key,
            Self::Network => SettingsTabIcon::Network,
            Self::Ide => SettingsTabIcon::Code2,
            Self::Ai => SettingsTabIcon::Sparkles,
            Self::Knowledge => SettingsTabIcon::BookOpen,
            Self::Keybindings => SettingsTabIcon::Keyboard,
            Self::Help => SettingsTabIcon::HelpCircle,
        }
    }
}

impl SettingsInput {
    pub fn accepts_newline(self) -> bool {
        // Keep multiline behavior beside the input identity so IME handling and
        // render controls cannot drift when new settings fields are added.
        matches!(
            self,
            Self::TerminalCommandBarFocusHandoff
                | Self::TerminalCommandSpecsJson
                | Self::AiSystemPrompt
                | Self::AiMemoryContent
                | Self::AiAcpAgentArgs(_)
                | Self::AiAcpAgentEnv(_)
                | Self::AiMcpArgs
                | Self::LocalPrivilegePromptPatterns
                | Self::ManagedKeyPastePrivateKey
        )
    }

    pub fn textarea_line_height(self) -> f32 {
        // These values describe settings text areas in logical pixels; GPUI
        // converts them to concrete units at the view boundary.
        match self {
            Self::TerminalCommandBarFocusHandoff | Self::TerminalCommandSpecsJson => 20.0,
            Self::AiSystemPrompt | Self::AiMemoryContent => 22.0,
            Self::AiAcpAgentArgs(_)
            | Self::AiAcpAgentEnv(_)
            | Self::AiMcpArgs
            | Self::LocalPrivilegePromptPatterns
            | Self::ManagedKeyPastePrivateKey => 20.0,
            _ => DEFAULT_SETTINGS_TEXTAREA_LINE_HEIGHT,
        }
    }

    pub fn anchor_key(self) -> u64 {
        match self {
            Self::TerminalCustomFontFamily => 19,
            Self::TerminalFontSize => 1,
            Self::TerminalScrollback => 33_000,
            Self::TerminalLineHeight => 2,
            Self::IdeFontSize => 3,
            Self::IdeLineHeight => 4,
            Self::AppearanceUiFont => 5,
            Self::LocalDefaultCwd => 6,
            Self::LocalGitBashPath => 7,
            Self::LocalOhMyPoshTheme => 8,
            Self::LocalPrivilegeLabel => 31_000,
            Self::LocalPrivilegeUsernameHint => 31_001,
            Self::LocalPrivilegeSecret => 31_002,
            Self::LocalPrivilegePromptPatterns => 31_003,
            Self::ConnectionDefaultUsername => 9,
            Self::ConnectionDefaultPort => 10,
            Self::ConnectionImportTargetGroup => 20,
            Self::NetworkProxyHost => 32_000,
            Self::NetworkProxyPort => 32_001,
            Self::NetworkProxyNoProxy => 32_002,
            Self::NetworkProxyUsername => 32_003,
            Self::NetworkProxyPassword => 32_004,
            Self::NetworkProxyTestHost => 32_005,
            Self::NetworkProxyTestPort => 32_006,
            Self::UpdateProxyHost => 32_100,
            Self::UpdateProxyPort => 32_101,
            Self::UpdateProxyNoProxy => 32_102,
            Self::SftpSpeedLimitKbps => 12,
            Self::InBandTransferMaxChunkBytes => 13,
            Self::InBandTransferMaxFileCount => 14,
            Self::InBandTransferMaxTotalBytes => 15,
            Self::TerminalCommandBarFocusHandoff => 16,
            Self::TerminalCommandSpecsJson => 17,
            Self::KeybindingSearch => 18,
            Self::CustomThemeName => 10_000,
            Self::CustomThemeTerminalColor(index) => 10_100 + index as u64,
            Self::CustomThemeUiColor(index) => 10_200 + index as u64,
            Self::HighlightLabel(index) => 100 + index as u64 * 4,
            Self::HighlightPattern(index) => 101 + index as u64 * 4,
            Self::HighlightForeground(index) => 102 + index as u64 * 4,
            Self::HighlightBackground(index) => 103 + index as u64 * 4,
            Self::AiProviderName(index) => 20_000 + index as u64 * 4,
            Self::AiProviderBaseUrl(index) => 20_001 + index as u64 * 4,
            Self::AiProviderDefaultModel(index) => 20_002 + index as u64 * 4,
            Self::AiProviderApiKey(index) => 20_003 + index as u64 * 4,
            Self::AiAcpAgentDisplayName(index) => 21_500 + index as u64 * 6,
            Self::AiAcpAgentCommand(index) => 21_501 + index as u64 * 6,
            Self::AiAcpAgentCwd(index) => 21_502 + index as u64 * 6,
            Self::AiAcpAgentArgs(index) => 21_503 + index as u64 * 6,
            Self::AiAcpAgentEnv(index) => 21_504 + index as u64 * 6,
            Self::AiAcpAgentAuthToken(index) => 21_505 + index as u64 * 6,
            Self::AiSystemPrompt => 22_000,
            Self::AiMemoryContent => 22_001,
            Self::AiToolUseMaxRounds => 22_002,
            Self::AiToolUseMaxCallsPerRound => 22_003,
            Self::AiModelContextWindow(provider_index, model_index) => {
                23_000 + provider_index as u64 * 1_000 + model_index as u64
            }
            Self::AiActiveModelMaxResponseTokens => 24_000,
            Self::AiEmbeddingModel => 24_001,
            Self::AiMcpName => 25_000,
            Self::AiMcpCommand => 25_001,
            Self::AiMcpArgs => 25_002,
            Self::AiMcpUrl => 25_003,
            Self::AiMcpAuthHeaderName => 25_004,
            Self::AiMcpAuthToken => 25_005,
            Self::AiMcpEnvKey(index) => 25_100 + index as u64 * 2,
            Self::AiMcpEnvValue(index) => 25_101 + index as u64 * 2,
            Self::AiMcpHeaderKey(index) => 25_300 + index as u64 * 2,
            Self::AiMcpHeaderValue(index) => 25_301 + index as u64 * 2,
            Self::KnowledgeCollectionName => 26_000,
            Self::KnowledgeDocumentTitle => 26_001,
            Self::CloudSyncEndpoint => 27_000,
            Self::CloudSyncNamespace => 27_001,
            Self::CloudSyncS3Bucket => 27_002,
            Self::CloudSyncS3Region => 27_003,
            Self::CloudSyncGitRepository => 27_004,
            Self::CloudSyncGitBranch => 27_005,
            Self::CloudSyncGithubOauthClientId => 27_006,
            Self::CloudSyncMicrosoftOauthClientId => 27_016,
            Self::CloudSyncGoogleOauthClientId => 27_017,
            Self::CloudSyncToken => 27_007,
            Self::CloudSyncGitToken => 27_008,
            Self::CloudSyncBasicUsername => 27_009,
            Self::CloudSyncBasicPassword => 27_010,
            Self::CloudSyncAccessKeyId => 27_011,
            Self::CloudSyncSecretAccessKey => 27_012,
            Self::CloudSyncSessionToken => 27_013,
            Self::CloudSyncSyncPassword => 27_014,
            Self::CloudSyncAutoUploadInterval => 27_015,
            Self::PortableCurrentPassword => 28_000,
            Self::PortableNewPassword => 28_001,
            Self::PortableConfirmPassword => 28_002,
            Self::AppLockCurrentPassword => 28_100,
            Self::AppLockNewPassword => 28_101,
            Self::AppLockConfirmPassword => 28_102,
            Self::NativePluginInstallUrl => PLUGIN_MANAGER_INPUT_ANCHOR_BASE,
            Self::NativePluginInstallChecksum => PLUGIN_MANAGER_INPUT_ANCHOR_BASE + 1,
            Self::NativePluginRegistryUrl => PLUGIN_MANAGER_INPUT_ANCHOR_BASE + 2,
            Self::ManagedKeyFilePath => 30_000,
            Self::ManagedKeyFileName => 30_001,
            Self::ManagedKeyFilePassphrase => 30_002,
            Self::ManagedKeyPasteName => 30_003,
            Self::ManagedKeyPastePrivateKey => 30_004,
            Self::ManagedKeyPastePassphrase => 30_005,
            Self::ManagedKeyRenameName => 30_006,
            Self::PluginSetting(index) => PLUGIN_SETTING_INPUT_ANCHOR_BASE + index as u64,
        }
    }

    pub fn is_secret(self) -> bool {
        matches!(
            self,
            Self::AiProviderApiKey(_)
                | Self::AiAcpAgentAuthToken(_)
                | Self::AiMcpAuthToken
                | Self::CloudSyncToken
                | Self::CloudSyncGitToken
                | Self::CloudSyncBasicUsername
                | Self::CloudSyncBasicPassword
                | Self::CloudSyncAccessKeyId
                | Self::CloudSyncSecretAccessKey
                | Self::CloudSyncSessionToken
                | Self::CloudSyncSyncPassword
                | Self::PortableCurrentPassword
                | Self::PortableNewPassword
                | Self::PortableConfirmPassword
                | Self::AppLockCurrentPassword
                | Self::AppLockNewPassword
                | Self::AppLockConfirmPassword
                | Self::LocalPrivilegeSecret
                | Self::ManagedKeyFilePassphrase
                | Self::ManagedKeyPastePrivateKey
                | Self::ManagedKeyPastePassphrase
                | Self::NetworkProxyPassword
        )
    }

    pub fn is_ai_mcp(self) -> bool {
        matches!(
            self,
            Self::AiMcpName
                | Self::AiMcpCommand
                | Self::AiMcpArgs
                | Self::AiMcpUrl
                | Self::AiMcpAuthHeaderName
                | Self::AiMcpAuthToken
                | Self::AiMcpEnvKey(_)
                | Self::AiMcpEnvValue(_)
                | Self::AiMcpHeaderKey(_)
                | Self::AiMcpHeaderValue(_)
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsTabIcon {
    BookOpen,
    Code2,
    HardDrive,
    HelpCircle,
    Key,
    Keyboard,
    Monitor,
    Network,
    Shield,
    Sparkles,
    Square,
    Terminal,
    WifiOff,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsBackgroundTabIcon {
    Activity,
    ArrowLeftRight,
    Bell,
    Cloud,
    Code2,
    Folder,
    FolderInput,
    Gauge,
    ListTree,
    Monitor,
    Network,
    Puzzle,
    Rocket,
    Settings,
    Terminal,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_input_anchor_ranges_do_not_overlap_dynamic_plugin_ranges() {
        assert!(SettingsInput::NativePluginInstallUrl.anchor_key() < 29_000);
        assert_eq!(SettingsInput::PluginSetting(0).anchor_key(), 29_000);
    }

    #[test]
    fn secret_inputs_are_categorized_in_the_model_layer() {
        assert!(SettingsInput::AiProviderApiKey(0).is_secret());
        assert!(SettingsInput::AiAcpAgentAuthToken(0).is_secret());
        assert!(SettingsInput::CloudSyncSecretAccessKey.is_secret());
        assert!(SettingsInput::PortableCurrentPassword.is_secret());
        assert!(SettingsInput::PortableNewPassword.is_secret());
        assert!(SettingsInput::PortableConfirmPassword.is_secret());
        assert!(SettingsInput::AppLockCurrentPassword.is_secret());
        assert!(SettingsInput::AppLockNewPassword.is_secret());
        assert!(SettingsInput::AppLockConfirmPassword.is_secret());
        assert!(SettingsInput::LocalPrivilegeSecret.is_secret());
        assert!(!SettingsInput::TerminalFontSize.is_secret());
    }

    #[test]
    fn ai_mcp_inputs_are_categorized_in_the_model_layer() {
        assert!(SettingsInput::AiMcpEnvValue(0).is_ai_mcp());
        assert!(!SettingsInput::AiSystemPrompt.is_ai_mcp());
    }

    #[test]
    fn multiline_input_metadata_lives_with_settings_input_identity() {
        assert!(SettingsInput::AiSystemPrompt.accepts_newline());
        assert!(SettingsInput::LocalPrivilegePromptPatterns.accepts_newline());
        assert!(!SettingsInput::TerminalFontSize.accepts_newline());
        assert_eq!(SettingsInput::AiMemoryContent.textarea_line_height(), 22.0);
    }

    #[test]
    fn settings_tabs_are_grouped_by_user_task() {
        assert_eq!(
            SettingsTab::groups(),
            &[
                &[
                    SettingsTab::General,
                    SettingsTab::Appearance,
                    SettingsTab::Keybindings,
                ][..],
                &[SettingsTab::Terminal, SettingsTab::Portable][..],
                &[
                    SettingsTab::Connections,
                    SettingsTab::Network,
                    SettingsTab::Sftp,
                    SettingsTab::Privilege,
                ][..],
                &[SettingsTab::Ide, SettingsTab::Ai, SettingsTab::Knowledge][..],
                &[SettingsTab::Help][..],
            ]
        );
        assert_eq!(
            SettingsTab::all(),
            SettingsTab::groups()
                .iter()
                .flat_map(|group| group.iter())
                .copied()
                .collect::<Vec<_>>()
        );
    }
}
