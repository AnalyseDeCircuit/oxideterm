// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! GPUI adapters for pure settings model types.
//!
//! The page model enums live in `oxideterm-settings-model`; this module keeps
//! only view-layer mapping to GPUI anchors.

use oxideterm_gpui_ui::select::SelectAnchorId;
pub use oxideterm_settings_model::{
    SettingsBackgroundTabIcon, SettingsInput, SettingsKeybindingScopeFilter, SettingsSelect,
    SettingsSlider, SettingsTab, SettingsTabIcon, TerminalSettingsPage,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActiveSurface {
    Terminal,
    Settings,
}

pub fn settings_tab_from_ai_section(section: &str) -> Option<SettingsTab> {
    match section {
        "general" => Some(SettingsTab::General),
        "portable" => Some(SettingsTab::Portable),
        "terminal" => Some(SettingsTab::Terminal),
        "appearance" => Some(SettingsTab::Appearance),
        "local" | "local_terminal" => Some(SettingsTab::Local),
        "connections" | "connection_manager" => Some(SettingsTab::Connections),
        "ssh" => Some(SettingsTab::Ssh),
        "reconnect" => Some(SettingsTab::Reconnect),
        "sftp" => Some(SettingsTab::Sftp),
        "ide" => Some(SettingsTab::Ide),
        "ai" | "assistant" => Some(SettingsTab::Ai),
        "knowledge" | "rag" => Some(SettingsTab::Knowledge),
        "keybindings" | "keyboard" => Some(SettingsTab::Keybindings),
        "help" => Some(SettingsTab::Help),
        _ => None,
    }
}

pub trait SettingsSelectAnchorExt {
    fn anchor_id(self) -> SelectAnchorId;
}

impl SettingsSelectAnchorExt for SettingsSelect {
    fn anchor_id(self) -> SelectAnchorId {
        match self {
            Self::Language => SelectAnchorId::SettingsLanguage,
            Self::UpdateChannel => SelectAnchorId::SettingsUpdateChannel,
            Self::UpdateProxyMode => SelectAnchorId::SettingsUpdateProxyMode,
            Self::UpdateProxyProtocol => SelectAnchorId::SettingsUpdateProxyProtocol,
            Self::AppearanceTheme => SelectAnchorId::SettingsAppearanceTheme,
            Self::AppearanceDensity => SelectAnchorId::SettingsAppearanceDensity,
            Self::AppearanceAnimation => SelectAnchorId::SettingsAppearanceAnimation,
            Self::AppearanceRenderProfile => SelectAnchorId::SettingsAppearanceRenderProfile,
            Self::AppearanceFrostedGlass => SelectAnchorId::SettingsAppearanceFrostedGlass,
            Self::AppearanceBackgroundFit => SelectAnchorId::SettingsAppearanceBackgroundFit,
            Self::CustomThemeDuplicate => SelectAnchorId::SettingsCustomThemeDuplicate,
            Self::TerminalFontFamily => SelectAnchorId::SettingsTerminalFontFamily,
            Self::TerminalEncoding => SelectAnchorId::SettingsTerminalEncoding,
            Self::TerminalCursorStyle => SelectAnchorId::SettingsTerminalCursorStyle,
            Self::IdeAgentMode => SelectAnchorId::SettingsIdeAgentMode,
            Self::LocalShell => SelectAnchorId::SettingsLocalShell,
            Self::LocalPrivilegeKind => SelectAnchorId::SettingsLocalPrivilegeKind,
            Self::ConnectionIdleTimeout => SelectAnchorId::SettingsConnectionIdleTimeout,
            Self::ReconnectMaxAttempts => SelectAnchorId::SettingsReconnectMaxAttempts,
            Self::ReconnectBaseDelay => SelectAnchorId::SettingsReconnectBaseDelay,
            Self::ReconnectMaxDelay => SelectAnchorId::SettingsReconnectMaxDelay,
            Self::NetworkProxyProtocol => SelectAnchorId::SettingsNetworkProxyProtocol,
            Self::NetworkProxyAuth => SelectAnchorId::SettingsNetworkProxyAuth,
            Self::AiProviderTemplate => SelectAnchorId::SettingsAiProviderTemplate,
            Self::AiContextMaxChars => SelectAnchorId::SettingsAiContextMaxChars,
            Self::AiContextVisibleLines => SelectAnchorId::SettingsAiContextVisibleLines,
            Self::AiGlobalReasoning => SelectAnchorId::SettingsAiGlobalReasoning,
            Self::AiProfileBackend(index) => SelectAnchorId::SettingsAiProfileBackend(index),
            Self::AiProfileProvider(index) => SelectAnchorId::SettingsAiProfileProvider(index),
            Self::AiProfileAcpAgent(index) => SelectAnchorId::SettingsAiProfileAcpAgent(index),
            Self::AiProfileReasoning(index) => SelectAnchorId::SettingsAiProfileReasoning(index),
            Self::AiProviderReasoning(index) => SelectAnchorId::SettingsAiProviderReasoning(index),
            Self::AiModelReasoning(provider_index, model_index) => {
                SelectAnchorId::SettingsAiModelReasoning(provider_index, model_index)
            }
            Self::AiEmbeddingProvider => SelectAnchorId::SettingsAiEmbeddingProvider,
            Self::KnowledgeCollectionScope => SelectAnchorId::SettingsKnowledgeCollectionScope,
            Self::KnowledgeDocumentFormat => SelectAnchorId::SettingsKnowledgeDocumentFormat,
            Self::AiMcpTransport => SelectAnchorId::SettingsAiMcpTransport,
            Self::AiMcpAuthMode => SelectAnchorId::SettingsAiMcpAuthMode,
            Self::SftpConcurrent => SelectAnchorId::SettingsSftpConcurrent,
            Self::SftpDirectoryParallelism => SelectAnchorId::SettingsSftpDirectoryParallelism,
            Self::SftpConflict => SelectAnchorId::SettingsSftpConflict,
            Self::HighlightPreset => SelectAnchorId::SettingsHighlightPreset,
            Self::HighlightRenderMode(index) => SelectAnchorId::SettingsHighlightRenderMode(index),
            Self::ConnectionImportSource => SelectAnchorId::SettingsConnectionImportSource,
            Self::ConnectionImportDuplicateStrategy => {
                SelectAnchorId::SettingsConnectionImportDuplicateStrategy
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ai_section_aliases_map_to_settings_tabs() {
        assert_eq!(
            settings_tab_from_ai_section("local_terminal"),
            Some(SettingsTab::Local)
        );
        assert_eq!(
            settings_tab_from_ai_section("assistant"),
            Some(SettingsTab::Ai)
        );
        assert_eq!(
            settings_tab_from_ai_section("keyboard"),
            Some(SettingsTab::Keybindings)
        );
        assert_eq!(settings_tab_from_ai_section("missing"), None);
    }
}
