#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ActiveSurface {
    Terminal,
    Settings,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SettingsTab {
    General,
    Portable,
    Terminal,
    Appearance,
    Local,
    Connections,
    Ssh,
    Reconnect,
    Sftp,
    Ide,
    Ai,
    Knowledge,
    Keybindings,
    Help,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TerminalSettingsPage {
    Display,
    Input,
    CommandBar,
    History,
    Transfer,
    Highlight,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SettingsSelect {
    Language,
    AppearanceTheme,
    AppearanceDensity,
    AppearanceAnimation,
    AppearanceRenderProfile,
    AppearanceFrostedGlass,
    AppearanceBackgroundFit,
    TerminalFontFamily,
    TerminalEncoding,
    TerminalAdaptiveRenderer,
    TerminalCursorStyle,
    LocalShell,
    HighlightPreset,
    HighlightRenderMode(usize),
}

impl SettingsSelect {
    fn anchor_id(self) -> SelectAnchorId {
        match self {
            Self::Language => SelectAnchorId::SettingsLanguage,
            Self::AppearanceTheme => SelectAnchorId::SettingsAppearanceTheme,
            Self::AppearanceDensity => SelectAnchorId::SettingsAppearanceDensity,
            Self::AppearanceAnimation => SelectAnchorId::SettingsAppearanceAnimation,
            Self::AppearanceRenderProfile => SelectAnchorId::SettingsAppearanceRenderProfile,
            Self::AppearanceFrostedGlass => SelectAnchorId::SettingsAppearanceFrostedGlass,
            Self::AppearanceBackgroundFit => SelectAnchorId::SettingsAppearanceBackgroundFit,
            Self::TerminalFontFamily => SelectAnchorId::SettingsTerminalFontFamily,
            Self::TerminalEncoding => SelectAnchorId::SettingsTerminalEncoding,
            Self::TerminalAdaptiveRenderer => SelectAnchorId::SettingsTerminalAdaptiveRenderer,
            Self::TerminalCursorStyle => SelectAnchorId::SettingsTerminalCursorStyle,
            Self::LocalShell => SelectAnchorId::SettingsLocalShell,
            Self::HighlightPreset => SelectAnchorId::SettingsHighlightPreset,
            Self::HighlightRenderMode(index) => SelectAnchorId::SettingsHighlightRenderMode(index),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(super) enum SettingsInput {
    TerminalFontSize,
    TerminalLineHeight,
    AppearanceUiFont,
    LocalDefaultCwd,
    LocalOhMyPoshTheme,
    HighlightLabel(usize),
    HighlightPattern(usize),
    HighlightForeground(usize),
    HighlightBackground(usize),
}

impl SettingsInput {
    pub(super) fn anchor_key(self) -> u64 {
        match self {
            Self::TerminalFontSize => 1,
            Self::TerminalLineHeight => 2,
            Self::AppearanceUiFont => 3,
            Self::LocalDefaultCwd => 4,
            Self::LocalOhMyPoshTheme => 5,
            Self::HighlightLabel(index) => 100 + index as u64 * 4,
            Self::HighlightPattern(index) => 101 + index as u64 * 4,
            Self::HighlightForeground(index) => 102 + index as u64 * 4,
            Self::HighlightBackground(index) => 103 + index as u64 * 4,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SettingsSlider {
    TerminalFontSize,
    AppearanceBorderRadius,
    AppearanceBackgroundOpacity,
    AppearanceBackgroundBlur,
}

impl TerminalSettingsPage {
    fn all() -> &'static [Self] {
        &[
            Self::Display,
            Self::Input,
            Self::CommandBar,
            Self::History,
            Self::Transfer,
            Self::Highlight,
        ]
    }

    fn label_key(self) -> &'static str {
        match self {
            Self::Display => "settings_view.terminal.page_display",
            Self::Input => "settings_view.terminal.page_input",
            Self::CommandBar => "settings_view.terminal.page_commandBar",
            Self::History => "settings_view.terminal.page_history",
            Self::Transfer => "settings_view.terminal.page_transfer",
            Self::Highlight => "settings_view.terminal.page_highlight",
        }
    }
}

impl SettingsTab {
    fn groups() -> &'static [&'static [Self]] {
        &[
            &[Self::General, Self::Portable],
            &[Self::Terminal, Self::Appearance, Self::Local],
            &[Self::Connections, Self::Ssh, Self::Reconnect],
            &[
                Self::Sftp,
                Self::Ide,
                Self::Ai,
                Self::Knowledge,
                Self::Keybindings,
            ],
            &[Self::Help],
        ]
    }

    fn label_key(self) -> &'static str {
        match self {
            Self::General => "settings.general.title",
            Self::Portable => "settings_view.general.portable_runtime",
            Self::Terminal => "settings.terminal.title",
            Self::Appearance => "settings_view.tabs.appearance",
            Self::Local => "settings_view.tabs.local",
            Self::Connections => "settings_view.tabs.connections",
            Self::Ssh => "settings_view.tabs.ssh",
            Self::Reconnect => "settings_view.tabs.reconnect",
            Self::Sftp => "settings_view.tabs.sftp",
            Self::Ide => "settings_view.tabs.ide",
            Self::Ai => "settings_view.tabs.ai",
            Self::Knowledge => "settings_view.tabs.knowledge",
            Self::Keybindings => "settings_view.tabs.keybindings",
            Self::Help => "settings_view.tabs.help",
        }
    }

    fn title_key(self) -> &'static str {
        match self {
            Self::General => "settings_view.general.title",
            Self::Portable => "settings_view.general.portable_runtime",
            Self::Terminal => "settings_view.terminal.title",
            Self::Appearance => "settings_view.appearance.title",
            Self::Local => "settings_view.local_terminal.title",
            Self::Connections => "settings_view.connections.title",
            Self::Ssh => "settings_view.tabs.ssh",
            Self::Reconnect => "settings_view.reconnect.title",
            Self::Sftp => "settings_view.sftp.title",
            Self::Ide => "settings_view.ide.title",
            Self::Ai => "settings_view.ai.title",
            Self::Knowledge => "settings_view.knowledge.title",
            Self::Keybindings => "settings_view.keybindings.title",
            Self::Help => "settings_view.help.title",
        }
    }

    fn description_key(self) -> &'static str {
        match self {
            Self::General => "settings_view.general.description",
            Self::Portable => "settings_view.general.portable_runtime_disabled_hint",
            Self::Terminal => "settings_view.terminal.description",
            Self::Appearance => "settings_view.appearance.description",
            Self::Local => "settings_view.local_terminal.description",
            Self::Connections => "settings_view.connections.description",
            Self::Ssh => "ssh.form.subtitle",
            Self::Reconnect => "settings_view.reconnect.description",
            Self::Sftp => "settings_view.sftp.description",
            Self::Ide => "settings_view.ide.description",
            Self::Ai => "settings_view.ai.description",
            Self::Knowledge => "settings_view.knowledge.description",
            Self::Keybindings => "settings_view.keybindings.description",
            Self::Help => "settings_view.help.description",
        }
    }

    fn icon(self) -> LucideIcon {
        match self {
            Self::General | Self::Appearance => LucideIcon::Monitor,
            Self::Portable | Self::Sftp => LucideIcon::HardDrive,
            Self::Local => LucideIcon::Square,
            Self::Terminal => LucideIcon::Terminal,
            Self::Connections => LucideIcon::Shield,
            Self::Ssh => LucideIcon::Key,
            Self::Reconnect => LucideIcon::WifiOff,
            Self::Ide => LucideIcon::Code2,
            Self::Ai => LucideIcon::Sparkles,
            Self::Knowledge => LucideIcon::BookOpen,
            Self::Keybindings => LucideIcon::Keyboard,
            Self::Help => LucideIcon::HelpCircle,
        }
    }
}
