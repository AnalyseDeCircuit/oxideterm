// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Settings page navigation and virtual-section model rules.
//!
//! The GPUI app supplies runtime counts for data-backed pages, while this
//! module owns the invariant section-count math shared by the settings page.

use std::collections::HashSet;

use crate::{AiSettingsPage, SettingsTab, TerminalSettingsPage};

pub const SETTINGS_SECTION_HEADER_ITEM_COUNT: usize = 1;

/// Stable semantic identifier for every virtualized settings card/section.
///
/// Rendering order is defined in one place (`settings_tab_sections`) so card
/// reordering, search navigation, scroll targeting, and list counts stay
/// decoupled from hardcoded indices.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SettingsSectionId {
    GeneralStartup,
    GeneralWindowBehavior,
    GeneralAppLock,
    GeneralLanguage,
    GeneralExternalConnectionUris,
    GeneralDataDirectory,
    GeneralCliCompanion,
    PortableRuntime,
    AppearanceTheme,
    AppearanceBackground,
    AppearanceLayout,
    AppearanceEffects,
    AppearanceAppIcon,
    TerminalPageSwitcher,
    TerminalDisplayFont,
    TerminalDisplayBehavior,
    TerminalDisplayCursor,
    TerminalDisplayCommandMarks,
    TerminalDisplayBuffer,
    TerminalInputClipboard,
    TerminalInputMouseLinks,
    TerminalInputKeyboard,
    TerminalLocalShell,
    TerminalLocalShellProfile,
    TerminalLocalPrivilegeCredentials,
    TerminalTransferInBand,
    TerminalTransferXmodem,
    TerminalCommandBarControls,
    TerminalCommandBarFocusHandoff,
    TerminalCommandBarQuickCommands,
    TerminalAwarenessControls,
    TerminalAwarenessShellIntegration,
    TerminalAwarenessTriggers,
    TerminalLoggingSession,
    TerminalHighlightSelection,
    TerminalHighlightSemantic,
    TerminalHighlightRules,
    ConnectionsDefaults,
    ConnectionsIdleTimeout,
    ConnectionsReconnect,
    ConnectionsSshKeys,
    ConnectionsKnownHosts,
    ConnectionsSshConfigImport,
    ConnectionsImporters,
    PrivilegeCredentials,
    PrivilegeTotp,
    NetworkSharedProxy,
    NetworkRouting,
    NetworkPublicMcp,
    SftpOpening,
    SftpPerformance,
    SftpConflict,
    IdeEditing,
    IdeTypography,
    IdeAgent,
    IdeAgentPrivacy,
    AiPageSwitcher,
    AiGeneralSettings,
    AiGeneralPrivacy,
    AiProvidersSettings,
    AiContextControls,
    AiContextSystemPrompt,
    AiContextMemory,
    AiContextModelWindows,
    AiToolsToolUse,
    AiToolsWebSearch,
    AiToolsAgents,
    AiToolsSkills,
    AiToolsMcpServers,
    KnowledgeError,
    KnowledgeCollections,
    KnowledgeEmbeddingConfig,
    KnowledgeDocuments,
    KeybindingsToolbar,
    KeybindingsScope(usize),
    HelpVersion,
    HelpDiagnostics,
    HelpTechStack,
    HelpResources,
    HelpSafety,
    HelpLegal,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsNavigationLayout {
    groups: Vec<Vec<SettingsTab>>,
}

impl Default for SettingsNavigationLayout {
    fn default() -> Self {
        Self::from_groups(SettingsTab::groups())
    }
}

impl SettingsNavigationLayout {
    pub fn from_persisted_groups(persisted_groups: &[Vec<String>]) -> Self {
        if persisted_groups.is_empty() {
            return Self::default();
        }

        let mut groups = Vec::with_capacity(persisted_groups.len());
        let mut seen = HashSet::new();

        for persisted_group in persisted_groups {
            let mut group = Vec::with_capacity(persisted_group.len());
            for id in persisted_group {
                if let Some(tab) = SettingsTab::from_id(id)
                    && seen.insert(tab)
                {
                    group.push(tab);
                }
            }
            if !group.is_empty() {
                groups.push(group);
            }
        }

        if groups.is_empty() {
            return Self::default();
        }

        // New settings pages remain reachable when an older custom layout is loaded.
        for tab in SettingsTab::all() {
            if seen.insert(*tab) {
                groups
                    .last_mut()
                    .expect("validated navigation layout has a group")
                    .push(*tab);
            }
        }

        Self { groups }
    }

    pub fn groups(&self) -> &[Vec<SettingsTab>] {
        &self.groups
    }

    pub fn group_count(&self) -> usize {
        self.groups.len()
    }

    pub fn add_group(&mut self) {
        self.groups.push(Vec::new());
    }

    pub fn remove_empty_group(&mut self, group_index: usize) -> bool {
        if self.groups.len() <= 1
            || self
                .groups
                .get(group_index)
                .is_none_or(|group| !group.is_empty())
        {
            return false;
        }
        self.groups.remove(group_index);
        true
    }

    pub fn move_tab_to_position(&mut self, tab: SettingsTab, target: SettingsTab) -> bool {
        if tab == target {
            return false;
        }
        let Some((target_group_index, target_index)) = self.tab_position(target) else {
            return false;
        };
        let Some(tab) = self.remove_tab(tab) else {
            return false;
        };
        let insertion_index = target_index.min(self.groups[target_group_index].len());
        self.groups[target_group_index].insert(insertion_index, tab);
        true
    }

    pub fn move_tab_to_group_start(&mut self, tab: SettingsTab, group_index: usize) -> bool {
        if group_index >= self.groups.len() {
            return false;
        }
        let Some(tab) = self.remove_tab(tab) else {
            return false;
        };
        self.groups[group_index].insert(0, tab);
        true
    }

    pub fn move_tab_to_group_end(&mut self, tab: SettingsTab, group_index: usize) -> bool {
        if group_index >= self.groups.len() {
            return false;
        }
        let Some(tab) = self.remove_tab(tab) else {
            return false;
        };
        self.groups[group_index].push(tab);
        true
    }

    pub fn move_group_to_position(&mut self, source_index: usize, target_index: usize) -> bool {
        if source_index == target_index
            || source_index >= self.groups.len()
            || target_index >= self.groups.len()
        {
            return false;
        }
        let group = self.groups.remove(source_index);
        let insertion_index = target_index.min(self.groups.len());
        self.groups.insert(insertion_index, group);
        true
    }

    pub fn move_group_to_end(&mut self, source_index: usize) -> bool {
        if source_index + 1 >= self.groups.len() {
            return false;
        }
        let group = self.groups.remove(source_index);
        self.groups.push(group);
        true
    }

    pub fn to_persisted_groups(&self) -> Vec<Vec<String>> {
        self.groups
            .iter()
            .filter(|group| !group.is_empty())
            .map(|group| group.iter().map(|tab| tab.id().to_string()).collect())
            .collect()
    }

    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    fn from_groups(groups: &[&[SettingsTab]]) -> Self {
        Self {
            groups: groups.iter().map(|group| group.to_vec()).collect(),
        }
    }

    fn tab_position(&self, tab: SettingsTab) -> Option<(usize, usize)> {
        self.groups
            .iter()
            .enumerate()
            .find_map(|(group_index, group)| {
                group
                    .iter()
                    .position(|candidate| *candidate == tab)
                    .map(|tab_index| (group_index, tab_index))
            })
    }

    fn remove_tab(&mut self, tab: SettingsTab) -> Option<SettingsTab> {
        let (group_index, tab_index) = self.tab_position(tab)?;
        Some(self.groups[group_index].remove(tab_index))
    }
}

pub fn terminal_settings_sections(page: TerminalSettingsPage) -> &'static [SettingsSectionId] {
    match page {
        TerminalSettingsPage::Display => &[
            SettingsSectionId::TerminalPageSwitcher,
            SettingsSectionId::TerminalDisplayFont,
            SettingsSectionId::TerminalDisplayBehavior,
            SettingsSectionId::TerminalDisplayCursor,
            SettingsSectionId::TerminalDisplayCommandMarks,
            SettingsSectionId::TerminalDisplayBuffer,
        ],
        TerminalSettingsPage::Input => &[
            SettingsSectionId::TerminalPageSwitcher,
            SettingsSectionId::TerminalInputClipboard,
            SettingsSectionId::TerminalInputMouseLinks,
            SettingsSectionId::TerminalInputKeyboard,
        ],
        // The dedicated keybindings page owns shortcut discovery and editing.
        TerminalSettingsPage::Local => &[
            SettingsSectionId::TerminalPageSwitcher,
            SettingsSectionId::TerminalLocalShell,
            SettingsSectionId::TerminalLocalShellProfile,
            SettingsSectionId::TerminalLocalPrivilegeCredentials,
        ],
        TerminalSettingsPage::Transfer => &[
            SettingsSectionId::TerminalPageSwitcher,
            SettingsSectionId::TerminalTransferInBand,
            SettingsSectionId::TerminalTransferXmodem,
        ],
        TerminalSettingsPage::CommandBar => &[
            SettingsSectionId::TerminalPageSwitcher,
            SettingsSectionId::TerminalCommandBarControls,
            SettingsSectionId::TerminalCommandBarFocusHandoff,
            SettingsSectionId::TerminalCommandBarQuickCommands,
        ],
        TerminalSettingsPage::Awareness => &[
            SettingsSectionId::TerminalPageSwitcher,
            SettingsSectionId::TerminalAwarenessControls,
            SettingsSectionId::TerminalAwarenessShellIntegration,
            SettingsSectionId::TerminalAwarenessTriggers,
        ],
        TerminalSettingsPage::Logging => &[
            SettingsSectionId::TerminalPageSwitcher,
            SettingsSectionId::TerminalLoggingSession,
        ],
        TerminalSettingsPage::Highlight => &[
            SettingsSectionId::TerminalPageSwitcher,
            SettingsSectionId::TerminalHighlightSelection,
            SettingsSectionId::TerminalHighlightSemantic,
            SettingsSectionId::TerminalHighlightRules,
        ],
    }
}

pub fn ai_settings_sections(page: AiSettingsPage) -> &'static [SettingsSectionId] {
    match page {
        // Feature activation and privacy guidance are independent cards.
        AiSettingsPage::General => &[
            SettingsSectionId::AiPageSwitcher,
            SettingsSectionId::AiGeneralSettings,
            SettingsSectionId::AiGeneralPrivacy,
        ],
        AiSettingsPage::Providers => &[
            SettingsSectionId::AiPageSwitcher,
            SettingsSectionId::AiProvidersSettings,
        ],
        // Context controls, prompt, memory, and model windows are
        // separate cards so each virtual row owns one stable responsibility.
        AiSettingsPage::Context => &[
            SettingsSectionId::AiPageSwitcher,
            SettingsSectionId::AiContextControls,
            SettingsSectionId::AiContextSystemPrompt,
            SettingsSectionId::AiContextMemory,
            SettingsSectionId::AiContextModelWindows,
        ],
        // Global tool policy, web access, subagents, skills, and MCP have separate cards.
        AiSettingsPage::Tools => &[
            SettingsSectionId::AiPageSwitcher,
            SettingsSectionId::AiToolsToolUse,
            SettingsSectionId::AiToolsWebSearch,
            SettingsSectionId::AiToolsAgents,
            SettingsSectionId::AiToolsSkills,
            SettingsSectionId::AiToolsMcpServers,
        ],
    }
}

pub fn settings_tab_sections(
    tab: SettingsTab,
    dynamic: SettingsDynamicSectionCounts,
) -> Vec<SettingsSectionId> {
    match tab {
        SettingsTab::General => {
            let mut sections = vec![SettingsSectionId::GeneralStartup];
            if cfg!(any(target_os = "windows", target_os = "macos")) {
                sections.push(SettingsSectionId::GeneralWindowBehavior);
            }
            sections.extend([
                SettingsSectionId::GeneralAppLock,
                SettingsSectionId::GeneralLanguage,
                SettingsSectionId::GeneralExternalConnectionUris,
                SettingsSectionId::GeneralDataDirectory,
                SettingsSectionId::GeneralCliCompanion,
            ]);
            sections
        }
        SettingsTab::Portable => vec![SettingsSectionId::PortableRuntime],
        SettingsTab::Terminal => terminal_settings_sections(dynamic.terminal_page).to_vec(),
        SettingsTab::Appearance => vec![
            SettingsSectionId::AppearanceTheme,
            SettingsSectionId::AppearanceBackground,
            SettingsSectionId::AppearanceLayout,
            SettingsSectionId::AppearanceEffects,
            SettingsSectionId::AppearanceAppIcon,
        ],
        // Reconnect controls share one card and therefore one virtual section.
        SettingsTab::Connections => vec![
            SettingsSectionId::ConnectionsDefaults,
            SettingsSectionId::ConnectionsIdleTimeout,
            SettingsSectionId::ConnectionsReconnect,
            SettingsSectionId::ConnectionsSshKeys,
            SettingsSectionId::ConnectionsKnownHosts,
            SettingsSectionId::ConnectionsSshConfigImport,
            SettingsSectionId::ConnectionsImporters,
        ],
        SettingsTab::Privilege => vec![
            SettingsSectionId::PrivilegeCredentials,
            SettingsSectionId::PrivilegeTotp,
        ],
        SettingsTab::Network => vec![
            SettingsSectionId::NetworkSharedProxy,
            SettingsSectionId::NetworkRouting,
            SettingsSectionId::NetworkPublicMcp,
        ],
        SettingsTab::Sftp => vec![
            SettingsSectionId::SftpOpening,
            SettingsSectionId::SftpPerformance,
            SettingsSectionId::SftpConflict,
        ],
        SettingsTab::Ide => vec![
            SettingsSectionId::IdeEditing,
            SettingsSectionId::IdeTypography,
            SettingsSectionId::IdeAgent,
            SettingsSectionId::IdeAgentPrivacy,
        ],
        SettingsTab::Ai => ai_settings_sections(dynamic.ai_page).to_vec(),
        SettingsTab::Knowledge => {
            let mut sections = Vec::with_capacity(4);
            if dynamic.knowledge_has_error {
                sections.push(SettingsSectionId::KnowledgeError);
            }
            sections.push(SettingsSectionId::KnowledgeCollections);
            sections.push(SettingsSectionId::KnowledgeEmbeddingConfig);
            if dynamic.knowledge_has_selected_collection {
                sections.push(SettingsSectionId::KnowledgeDocuments);
            }
            sections
        }
        SettingsTab::Keybindings => {
            let scope_count = dynamic.visible_keybinding_scope_count.max(1);
            let mut sections = Vec::with_capacity(1 + scope_count);
            sections.push(SettingsSectionId::KeybindingsToolbar);
            for scope_index in 0..scope_count {
                sections.push(SettingsSectionId::KeybindingsScope(scope_index));
            }
            sections
        }
        SettingsTab::Help => vec![
            SettingsSectionId::HelpVersion,
            SettingsSectionId::HelpDiagnostics,
            SettingsSectionId::HelpTechStack,
            SettingsSectionId::HelpResources,
            SettingsSectionId::HelpSafety,
            SettingsSectionId::HelpLegal,
        ],
    }
}

pub fn settings_tab_section_at(
    tab: SettingsTab,
    section_index: usize,
    dynamic: SettingsDynamicSectionCounts,
) -> Option<SettingsSectionId> {
    settings_tab_sections(tab, dynamic)
        .get(section_index)
        .copied()
}

pub fn settings_section_index(
    tab: SettingsTab,
    target: SettingsSectionId,
    dynamic: SettingsDynamicSectionCounts,
) -> Option<usize> {
    settings_tab_sections(tab, dynamic)
        .into_iter()
        .position(|section| section == target)
}

pub fn settings_section_list_item_index(
    tab: SettingsTab,
    target: SettingsSectionId,
    dynamic: SettingsDynamicSectionCounts,
) -> Option<usize> {
    settings_section_index(tab, target, dynamic)
        .map(|section_index| SETTINGS_SECTION_HEADER_ITEM_COUNT + section_index)
}

pub fn settings_section_list_item_count(
    tab: SettingsTab,
    dynamic: SettingsDynamicSectionCounts,
) -> usize {
    SETTINGS_SECTION_HEADER_ITEM_COUNT + settings_tab_section_count(tab, dynamic)
}

pub fn settings_tab_section_count(
    tab: SettingsTab,
    dynamic: SettingsDynamicSectionCounts,
) -> usize {
    match tab {
        SettingsTab::Terminal => terminal_settings_section_count(dynamic.terminal_page),
        SettingsTab::Ai => ai_settings_section_count(dynamic.ai_page),
        SettingsTab::Keybindings => {
            keybinding_settings_section_count(dynamic.visible_keybinding_scope_count)
        }
        SettingsTab::Knowledge => knowledge_settings_section_count(
            dynamic.knowledge_has_error,
            dynamic.knowledge_has_selected_collection,
        ),
        _ => settings_tab_sections(tab, dynamic).len(),
    }
}

pub fn terminal_settings_section_count(page: TerminalSettingsPage) -> usize {
    terminal_settings_sections(page).len()
}

pub fn ai_settings_section_count(page: AiSettingsPage) -> usize {
    ai_settings_sections(page).len()
}

pub fn keybinding_settings_section_count(visible_scope_count: usize) -> usize {
    1 + visible_scope_count.max(1)
}

pub fn knowledge_settings_section_count(has_error: bool, has_selected_collection: bool) -> usize {
    2 + usize::from(has_error) + usize::from(has_selected_collection)
}

pub fn settings_section_list_identity(
    tab: SettingsTab,
    terminal_page: TerminalSettingsPage,
    ai_page: AiSettingsPage,
) -> String {
    // Keybinding filters update row signatures rather than replacing the list;
    // this keeps the toolbar-mounted selection animation alive.
    format!("{tab:?}:{terminal_page:?}:{ai_page:?}")
}

#[derive(Clone, Copy, Debug)]
pub struct SettingsDynamicSectionCounts {
    pub terminal_page: TerminalSettingsPage,
    pub ai_page: AiSettingsPage,
    pub visible_keybinding_scope_count: usize,
    pub knowledge_has_error: bool,
    pub knowledge_has_selected_collection: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persisted_navigation_layout_ignores_invalid_entries_and_appends_new_tabs() {
        let persisted = vec![
            vec!["terminal".to_string(), "unknown".to_string()],
            vec!["general".to_string(), "terminal".to_string()],
        ];

        let layout = SettingsNavigationLayout::from_persisted_groups(&persisted);

        assert_eq!(layout.groups()[0], [SettingsTab::Terminal]);
        assert_eq!(layout.groups()[1][..1], [SettingsTab::General]);
        assert_eq!(
            layout.groups().iter().map(Vec::len).sum::<usize>(),
            SettingsTab::all().len()
        );
        assert_eq!(layout.groups().len(), 2);
    }

    #[test]
    fn navigation_pages_can_move_within_and_across_groups() {
        let mut layout = SettingsNavigationLayout::default();

        assert!(layout.move_tab_to_position(SettingsTab::Terminal, SettingsTab::Appearance));
        assert_eq!(
            layout.groups()[0][..3],
            [
                SettingsTab::General,
                SettingsTab::Terminal,
                SettingsTab::Appearance,
            ]
        );
        assert!(layout.move_tab_to_group_end(SettingsTab::General, 1));
        assert_eq!(layout.groups()[1].last(), Some(&SettingsTab::General));
    }

    #[test]
    fn navigation_groups_can_be_added_removed_and_reordered() {
        let mut layout = SettingsNavigationLayout::default();
        let original_first_group = layout.groups()[0].clone();

        layout.add_group();
        assert_eq!(layout.group_count(), 6);
        assert!(layout.move_group_to_end(0));
        assert_eq!(layout.groups().last(), Some(&original_first_group));
        assert!(layout.remove_empty_group(4));
        assert_eq!(layout.group_count(), 5);
    }

    #[test]
    fn default_groups_cover_every_tab_once_in_expected_domain_order() {
        let layout = SettingsNavigationLayout::default();
        assert_eq!(
            layout.groups(),
            &[
                vec![
                    SettingsTab::General,
                    SettingsTab::Appearance,
                    SettingsTab::Portable,
                ],
                vec![
                    SettingsTab::Terminal,
                    SettingsTab::Ide,
                    SettingsTab::Keybindings,
                ],
                vec![
                    SettingsTab::Connections,
                    SettingsTab::Sftp,
                    SettingsTab::Privilege,
                    SettingsTab::Network,
                ],
                vec![SettingsTab::Ai, SettingsTab::Knowledge],
                vec![SettingsTab::Help],
            ]
        );
        let flattened = layout
            .groups()
            .iter()
            .flat_map(|group| group.iter().copied())
            .collect::<Vec<_>>();
        assert_eq!(flattened, SettingsTab::all());
    }

    #[test]
    fn settings_sections_have_unique_ids_and_consistent_counts() {
        for terminal_page in TerminalSettingsPage::all() {
            for ai_page in AiSettingsPage::all() {
                for knowledge_has_error in [false, true] {
                    for knowledge_has_selected_collection in [false, true] {
                        let dynamic = SettingsDynamicSectionCounts {
                            terminal_page: *terminal_page,
                            ai_page: *ai_page,
                            visible_keybinding_scope_count: 3,
                            knowledge_has_error,
                            knowledge_has_selected_collection,
                        };
                        for tab in SettingsTab::all() {
                            let sections = settings_tab_sections(*tab, dynamic);
                            let unique = sections.iter().copied().collect::<HashSet<_>>();
                            assert_eq!(
                                unique.len(),
                                sections.len(),
                                "duplicate section in {tab:?}"
                            );
                            assert_eq!(
                                settings_tab_section_count(*tab, dynamic),
                                sections.len(),
                                "count mismatch in {tab:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn section_item_indices_track_dynamic_error_and_platform_offsets() {
        let without_error = SettingsDynamicSectionCounts {
            terminal_page: TerminalSettingsPage::Awareness,
            ai_page: AiSettingsPage::Tools,
            visible_keybinding_scope_count: 1,
            knowledge_has_error: false,
            knowledge_has_selected_collection: true,
        };
        let with_error = SettingsDynamicSectionCounts {
            knowledge_has_error: true,
            ..without_error
        };
        assert_eq!(
            settings_section_list_item_index(
                SettingsTab::Knowledge,
                SettingsSectionId::KnowledgeDocuments,
                without_error,
            ),
            Some(SETTINGS_SECTION_HEADER_ITEM_COUNT + 2)
        );
        assert_eq!(
            settings_section_list_item_index(
                SettingsTab::Knowledge,
                SettingsSectionId::KnowledgeDocuments,
                with_error,
            ),
            Some(SETTINGS_SECTION_HEADER_ITEM_COUNT + 3)
        );
        assert_eq!(
            settings_section_list_item_index(
                SettingsTab::Terminal,
                SettingsSectionId::TerminalAwarenessTriggers,
                without_error,
            ),
            Some(SETTINGS_SECTION_HEADER_ITEM_COUNT + 3)
        );
    }
}
