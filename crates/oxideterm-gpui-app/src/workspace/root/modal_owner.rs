// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum ActiveTabWindowModalKind {
    SettingsNavigationEditor,
    AiMcpServer,
    KnowledgeCollectionCreate,
    KnowledgeDocumentCreate,
    KnowledgeDelete,
    KeybindingReset,
    ManagedKey,
    PortablePassword,
    SessionManagerGroupManager,
    SessionManagerDelete,
    ForwardEdit,
    ForwardDelete,
    SftpEditor,
    SftpDialog,
    FileManagerDialog,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) struct ActiveTabWindowModalSnapshot {
    kind: ActiveTabWindowModalKind,
    phase: oxideterm_gpui_ui::motion::ExitPhase,
}

/// Identifies the top blocking portal that owns window keyboard input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum ActiveWindowModalOwner {
    NewConnection,
    LocalShellLauncher,
    JumpServer,
    HostKeyChallenge,
    KeyboardInteractiveChallenge,
    AiEnable {
        phase: oxideterm_gpui_ui::motion::ExitPhase,
    },
    AiProviderKeyRemove {
        phase: oxideterm_gpui_ui::motion::ExitPhase,
    },
    AiProviderRemove {
        phase: oxideterm_gpui_ui::motion::ExitPhase,
    },
    AiSafety {
        phase: oxideterm_gpui_ui::motion::ExitPhase,
    },
    AiSummarize {
        phase: oxideterm_gpui_ui::motion::ExitPhase,
    },
    AiClearAll {
        phase: oxideterm_gpui_ui::motion::ExitPhase,
    },
    AiDeleteMessage {
        phase: oxideterm_gpui_ui::motion::ExitPhase,
    },
    SettingsReset {
        phase: oxideterm_gpui_ui::motion::ExitPhase,
    },
    SettingsDataDirectory {
        phase: oxideterm_gpui_ui::motion::ExitPhase,
    },
    RemoteShellIntegration,
    TerminalTriggerQuickCommand,
    CloudSync {
        phase: oxideterm_gpui_ui::motion::ExitPhase,
    },
    NodeDisconnect {
        phase: oxideterm_gpui_ui::motion::ExitPhase,
    },
    TabClose {
        phase: oxideterm_gpui_ui::motion::ExitPhase,
    },
    HostProcessConfirm {
        phase: oxideterm_gpui_ui::motion::ExitPhase,
    },
    HostDockerConfirm {
        phase: oxideterm_gpui_ui::motion::ExitPhase,
    },
    HostDockerLogs,
    HostServiceConfirm {
        phase: oxideterm_gpui_ui::motion::ExitPhase,
    },
    HostServiceLogs,
    HostTmuxConfirm {
        phase: oxideterm_gpui_ui::motion::ExitPhase,
    },
    HostTmuxInput,
    HostScheduleConfirm {
        phase: oxideterm_gpui_ui::motion::ExitPhase,
    },
    HostScheduleLogs,
    NativePluginConfirm {
        phase: oxideterm_gpui_ui::motion::ExitPhase,
    },
    TabRename,
    ActiveTabWindowModal {
        kind: ActiveTabWindowModalKind,
        phase: oxideterm_gpui_ui::motion::ExitPhase,
    },
    TerminalCastPlayer,
    ThemeEditor {
        phase: oxideterm_gpui_ui::motion::ExitPhase,
    },
    SettingsSshConfigImport {
        phase: oxideterm_gpui_ui::motion::ExitPhase,
    },
    TerminalCommandSpecsEditor,
    QuickCommandsManager,
    AiTextEditor,
    OxideImport {
        phase: oxideterm_gpui_ui::motion::ExitPhase,
    },
    OxideExport {
        phase: oxideterm_gpui_ui::motion::ExitPhase,
    },
    CommandPalette,
    VersionMigration,
    Onboarding,
    LegalNotice {
        phase: oxideterm_gpui_ui::motion::ExitPhase,
    },
    NativeUpdateReleaseNotes {
        phase: oxideterm_gpui_ui::motion::ExitPhase,
    },
    Shortcuts,
    AppLockDialog,
    MermaidZoom,
}

impl ActiveWindowModalOwner {
    fn render_rank(self) -> u8 {
        // These ranks mirror the portal child order in root/render.rs. A later
        // child is visually above an earlier child and therefore owns keys.
        match self {
            Self::NewConnection => 0,
            Self::LocalShellLauncher => 1,
            Self::JumpServer => 2,
            Self::HostKeyChallenge => 3,
            Self::KeyboardInteractiveChallenge => 4,
            Self::AiEnable { .. } => 5,
            Self::AiProviderKeyRemove { .. } => 6,
            Self::AiProviderRemove { .. } => 7,
            Self::AiSafety { .. } => 8,
            Self::AiSummarize { .. } => 9,
            Self::AiClearAll { .. } => 10,
            Self::AiDeleteMessage { .. } => 11,
            Self::SettingsReset { .. } => 12,
            Self::SettingsDataDirectory { .. } => 13,
            Self::RemoteShellIntegration => 14,
            Self::TerminalTriggerQuickCommand => 15,
            Self::CloudSync { .. } => 16,
            Self::NodeDisconnect { .. } => 17,
            Self::TabClose { .. } => 18,
            Self::HostProcessConfirm { .. } => 19,
            Self::HostDockerConfirm { .. } => 20,
            Self::HostDockerLogs => 21,
            Self::HostServiceConfirm { .. } => 22,
            Self::HostServiceLogs => 23,
            Self::HostTmuxConfirm { .. } => 24,
            Self::HostTmuxInput => 25,
            Self::HostScheduleConfirm { .. } => 26,
            Self::HostScheduleLogs => 27,
            Self::NativePluginConfirm { .. } => 28,
            Self::TabRename => 29,
            Self::ActiveTabWindowModal { .. } => 30,
            Self::TerminalCastPlayer => 31,
            Self::ThemeEditor { .. } => 32,
            Self::SettingsSshConfigImport { .. } => 33,
            Self::TerminalCommandSpecsEditor => 34,
            Self::QuickCommandsManager => 35,
            Self::AiTextEditor => 36,
            Self::OxideImport { .. } => 37,
            Self::OxideExport { .. } => 38,
            Self::CommandPalette => 39,
            Self::VersionMigration => 40,
            Self::Onboarding => 41,
            Self::LegalNotice { .. } => 42,
            Self::NativeUpdateReleaseNotes { .. } => 43,
            Self::Shortcuts => 44,
            Self::AppLockDialog => 45,
            Self::MermaidZoom => 46,
        }
    }

    fn phase(self) -> oxideterm_gpui_ui::motion::ExitPhase {
        match self {
            Self::AiEnable { phase }
            | Self::AiProviderKeyRemove { phase }
            | Self::AiProviderRemove { phase }
            | Self::AiSafety { phase }
            | Self::AiSummarize { phase }
            | Self::AiClearAll { phase }
            | Self::AiDeleteMessage { phase }
            | Self::SettingsReset { phase }
            | Self::SettingsDataDirectory { phase }
            | Self::CloudSync { phase }
            | Self::NodeDisconnect { phase }
            | Self::TabClose { phase }
            | Self::HostProcessConfirm { phase }
            | Self::HostDockerConfirm { phase }
            | Self::HostServiceConfirm { phase }
            | Self::HostTmuxConfirm { phase }
            | Self::HostScheduleConfirm { phase }
            | Self::NativePluginConfirm { phase }
            | Self::ActiveTabWindowModal { phase, .. }
            | Self::ThemeEditor { phase }
            | Self::SettingsSshConfigImport { phase }
            | Self::OxideImport { phase }
            | Self::OxideExport { phase }
            | Self::LegalNotice { phase }
            | Self::NativeUpdateReleaseNotes { phase } => phase,
            Self::NewConnection
            | Self::LocalShellLauncher
            | Self::JumpServer
            | Self::HostKeyChallenge
            | Self::KeyboardInteractiveChallenge
            | Self::RemoteShellIntegration
            | Self::TerminalTriggerQuickCommand
            | Self::HostDockerLogs
            | Self::HostServiceLogs
            | Self::HostTmuxInput
            | Self::HostScheduleLogs
            | Self::TabRename
            | Self::TerminalCastPlayer
            | Self::TerminalCommandSpecsEditor
            | Self::QuickCommandsManager
            | Self::AiTextEditor
            | Self::CommandPalette
            | Self::VersionMigration
            | Self::Onboarding
            | Self::Shortcuts
            | Self::AppLockDialog
            | Self::MermaidZoom => oxideterm_gpui_ui::motion::ExitPhase::Visible,
        }
    }

    fn allows_modal_ime(self) -> bool {
        matches!(
            self,
            Self::NewConnection
                | Self::JumpServer
                | Self::KeyboardInteractiveChallenge
                | Self::HostTmuxInput
                | Self::HostDockerLogs
                | Self::HostServiceLogs
                | Self::HostScheduleLogs
                | Self::TabRename
                | Self::ActiveTabWindowModal { .. }
                | Self::TerminalCastPlayer
                | Self::ThemeEditor { .. }
                | Self::SettingsSshConfigImport { .. }
                | Self::TerminalCommandSpecsEditor
                | Self::QuickCommandsManager
                | Self::AiTextEditor
                | Self::OxideImport { .. }
                | Self::OxideExport { .. }
                | Self::CommandPalette
                | Self::AppLockDialog
        )
    }

    fn key_route(self, key: &str) -> ActiveWindowModalKeyRoute {
        let visible = self.phase() == oxideterm_gpui_ui::motion::ExitPhase::Visible;
        let focused_child_owns_key = visible
            && (self == Self::AiTextEditor
                || matches!(
                    self,
                    Self::ActiveTabWindowModal {
                        kind: ActiveTabWindowModalKind::SftpEditor,
                        ..
                    }
                ))
            && key != "escape";
        ActiveWindowModalKeyRoute {
            // Document editors must receive navigation and mutation keys at
            // their focused element; the modal keeps Escape at the capture layer.
            dispatch_owner: (visible && !focused_child_owns_key).then_some(self),
            consume_in_capture: !focused_child_owns_key,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ActiveWindowModalKeyRoute {
    dispatch_owner: Option<ActiveWindowModalOwner>,
    consume_in_capture: bool,
}

impl ActiveWindowModalKeyRoute {
    fn consumes_key(self) -> bool {
        self.consume_in_capture
    }
}

/// Read-only inputs used to select the top blocking portal for a frame.
#[derive(Clone, Copy, Debug, Default)]
pub(in crate::workspace) struct ActiveWindowModalProjection {
    pub(in crate::workspace) new_connection_open: bool,
    pub(in crate::workspace) local_shell_launcher_open: bool,
    pub(in crate::workspace) jump_server_open: bool,
    pub(in crate::workspace) host_key_challenge_open: bool,
    pub(in crate::workspace) keyboard_interactive_challenge_open: bool,
    pub(in crate::workspace) ai_enable_phase: Option<oxideterm_gpui_ui::motion::ExitPhase>,
    pub(in crate::workspace) ai_provider_key_remove_phase:
        Option<oxideterm_gpui_ui::motion::ExitPhase>,
    pub(in crate::workspace) ai_provider_remove_phase: Option<oxideterm_gpui_ui::motion::ExitPhase>,
    pub(in crate::workspace) ai_safety_phase: Option<oxideterm_gpui_ui::motion::ExitPhase>,
    pub(in crate::workspace) ai_summarize_phase: Option<oxideterm_gpui_ui::motion::ExitPhase>,
    pub(in crate::workspace) ai_confirm: Option<ai_state::AiChatConfirmOwnerSnapshot>,
    pub(in crate::workspace) overlay_confirm: Option<overlay::WorkspaceOverlayConfirmOwnerSnapshot>,
    pub(in crate::workspace) settings_data_directory_phase:
        Option<oxideterm_gpui_ui::motion::ExitPhase>,
    pub(in crate::workspace) remote_shell_integration_open: bool,
    pub(in crate::workspace) terminal_trigger_quick_command_open: bool,
    pub(in crate::workspace) cloud_sync_phase: Option<oxideterm_gpui_ui::motion::ExitPhase>,
    pub(in crate::workspace) tab_close_phase: Option<oxideterm_gpui_ui::motion::ExitPhase>,
    pub(in crate::workspace) host_tools_modal:
        Option<connection_monitor::HostToolsWindowModalSnapshot>,
    pub(in crate::workspace) native_plugin_phase: Option<oxideterm_gpui_ui::motion::ExitPhase>,
    pub(in crate::workspace) tab_rename_open: bool,
    pub(in crate::workspace) active_tab_modal: Option<ActiveTabWindowModalSnapshot>,
    pub(in crate::workspace) terminal_cast_player_open: bool,
    pub(in crate::workspace) theme_editor_phase: Option<oxideterm_gpui_ui::motion::ExitPhase>,
    pub(in crate::workspace) settings_ssh_import_phase:
        Option<oxideterm_gpui_ui::motion::ExitPhase>,
    pub(in crate::workspace) terminal_command_specs_editor_open: bool,
    pub(in crate::workspace) quick_commands_manager_open: bool,
    pub(in crate::workspace) ai_text_editor_open: bool,
    pub(in crate::workspace) oxide_import_phase: Option<oxideterm_gpui_ui::motion::ExitPhase>,
    pub(in crate::workspace) oxide_export_phase: Option<oxideterm_gpui_ui::motion::ExitPhase>,
    pub(in crate::workspace) command_palette_open: bool,
    pub(in crate::workspace) version_migration_open: bool,
    pub(in crate::workspace) onboarding_open: bool,
    pub(in crate::workspace) shortcuts_open: bool,
    pub(in crate::workspace) app_lock_dialog_open: bool,
    pub(in crate::workspace) mermaid_zoom_open: bool,
    pub(in crate::workspace) native_update_toast_visible: bool,
}

impl ActiveWindowModalProjection {
    pub(in crate::workspace) fn top_owner(self) -> Option<ActiveWindowModalOwner> {
        let new_connection_owner = self
            .new_connection_open
            .then_some(ActiveWindowModalOwner::NewConnection);
        let local_shell_owner = self
            .local_shell_launcher_open
            .then_some(ActiveWindowModalOwner::LocalShellLauncher);
        let jump_server_owner = self
            .jump_server_open
            .then_some(ActiveWindowModalOwner::JumpServer);
        let host_key_owner = self
            .host_key_challenge_open
            .then_some(ActiveWindowModalOwner::HostKeyChallenge);
        let keyboard_interactive_owner = self
            .keyboard_interactive_challenge_open
            .then_some(ActiveWindowModalOwner::KeyboardInteractiveChallenge);
        let ai_enable_owner = self
            .ai_enable_phase
            .map(|phase| ActiveWindowModalOwner::AiEnable { phase });
        let ai_provider_key_remove_owner = self
            .ai_provider_key_remove_phase
            .map(|phase| ActiveWindowModalOwner::AiProviderKeyRemove { phase });
        let ai_provider_remove_owner = self
            .ai_provider_remove_phase
            .map(|phase| ActiveWindowModalOwner::AiProviderRemove { phase });
        let ai_safety_owner = self
            .ai_safety_phase
            .map(|phase| ActiveWindowModalOwner::AiSafety { phase });
        let ai_summarize_owner = self
            .ai_summarize_phase
            .map(|phase| ActiveWindowModalOwner::AiSummarize { phase });
        let ai_owner = self.ai_confirm.map(|snapshot| match snapshot.kind {
            ai_state::AiChatConfirmOwnerKind::ClearAll => ActiveWindowModalOwner::AiClearAll {
                phase: snapshot.phase,
            },
            ai_state::AiChatConfirmOwnerKind::DeleteMessage => {
                ActiveWindowModalOwner::AiDeleteMessage {
                    phase: snapshot.phase,
                }
            }
        });
        let overlay_owner = self.overlay_confirm.map(|snapshot| match snapshot.kind {
            overlay::WorkspaceOverlayConfirmOwnerKind::SettingsReset => {
                ActiveWindowModalOwner::SettingsReset {
                    phase: snapshot.phase,
                }
            }
            overlay::WorkspaceOverlayConfirmOwnerKind::LegalNotice => {
                ActiveWindowModalOwner::LegalNotice {
                    phase: snapshot.phase,
                }
            }
            overlay::WorkspaceOverlayConfirmOwnerKind::NativeUpdateReleaseNotes => {
                ActiveWindowModalOwner::NativeUpdateReleaseNotes {
                    phase: snapshot.phase,
                }
            }
            overlay::WorkspaceOverlayConfirmOwnerKind::NodeDisconnect => {
                ActiveWindowModalOwner::NodeDisconnect {
                    phase: snapshot.phase,
                }
            }
        });
        let tab_owner = self
            .tab_close_phase
            .map(|phase| ActiveWindowModalOwner::TabClose { phase });
        let settings_data_owner = self
            .settings_data_directory_phase
            .map(|phase| ActiveWindowModalOwner::SettingsDataDirectory { phase });
        let remote_shell_owner = self
            .remote_shell_integration_open
            .then_some(ActiveWindowModalOwner::RemoteShellIntegration);
        let terminal_trigger_owner = self
            .terminal_trigger_quick_command_open
            .then_some(ActiveWindowModalOwner::TerminalTriggerQuickCommand);
        let cloud_sync_owner = self
            .cloud_sync_phase
            .map(|phase| ActiveWindowModalOwner::CloudSync { phase });
        let host_tools_owner = self.host_tools_modal.map(|snapshot| match snapshot {
            connection_monitor::HostToolsWindowModalSnapshot::ProcessConfirm(phase) => {
                ActiveWindowModalOwner::HostProcessConfirm { phase }
            }
            connection_monitor::HostToolsWindowModalSnapshot::DockerConfirm(phase) => {
                ActiveWindowModalOwner::HostDockerConfirm { phase }
            }
            connection_monitor::HostToolsWindowModalSnapshot::DockerLogs => {
                ActiveWindowModalOwner::HostDockerLogs
            }
            connection_monitor::HostToolsWindowModalSnapshot::ServiceConfirm(phase) => {
                ActiveWindowModalOwner::HostServiceConfirm { phase }
            }
            connection_monitor::HostToolsWindowModalSnapshot::ServiceLogs => {
                ActiveWindowModalOwner::HostServiceLogs
            }
            connection_monitor::HostToolsWindowModalSnapshot::TmuxConfirm(phase) => {
                ActiveWindowModalOwner::HostTmuxConfirm { phase }
            }
            connection_monitor::HostToolsWindowModalSnapshot::TmuxInput => {
                ActiveWindowModalOwner::HostTmuxInput
            }
            connection_monitor::HostToolsWindowModalSnapshot::ScheduleConfirm(phase) => {
                ActiveWindowModalOwner::HostScheduleConfirm { phase }
            }
            connection_monitor::HostToolsWindowModalSnapshot::ScheduleLogs => {
                ActiveWindowModalOwner::HostScheduleLogs
            }
        });
        let native_plugin_owner = self
            .native_plugin_phase
            .map(|phase| ActiveWindowModalOwner::NativePluginConfirm { phase });
        let tab_rename_owner = self
            .tab_rename_open
            .then_some(ActiveWindowModalOwner::TabRename);
        let active_tab_owner =
            self.active_tab_modal
                .map(|snapshot| ActiveWindowModalOwner::ActiveTabWindowModal {
                    kind: snapshot.kind,
                    phase: snapshot.phase,
                });
        let command_palette_owner = self
            .command_palette_open
            .then_some(ActiveWindowModalOwner::CommandPalette);
        let terminal_cast_owner = self
            .terminal_cast_player_open
            .then_some(ActiveWindowModalOwner::TerminalCastPlayer);
        let theme_editor_owner = self
            .theme_editor_phase
            .map(|phase| ActiveWindowModalOwner::ThemeEditor { phase });
        let ssh_import_owner = self
            .settings_ssh_import_phase
            .map(|phase| ActiveWindowModalOwner::SettingsSshConfigImport { phase });
        let command_specs_owner = self
            .terminal_command_specs_editor_open
            .then_some(ActiveWindowModalOwner::TerminalCommandSpecsEditor);
        let quick_commands_manager_owner = self
            .quick_commands_manager_open
            .then_some(ActiveWindowModalOwner::QuickCommandsManager);
        let ai_text_editor_owner = self
            .ai_text_editor_open
            .then_some(ActiveWindowModalOwner::AiTextEditor);
        let oxide_import_owner = self
            .oxide_import_phase
            .map(|phase| ActiveWindowModalOwner::OxideImport { phase });
        let oxide_export_owner = self
            .oxide_export_phase
            .map(|phase| ActiveWindowModalOwner::OxideExport { phase });
        let version_migration_owner = self
            .version_migration_open
            .then_some(ActiveWindowModalOwner::VersionMigration);
        let onboarding_owner = (self.onboarding_open && !self.version_migration_open)
            .then_some(ActiveWindowModalOwner::Onboarding);
        let shortcuts_owner = self
            .shortcuts_open
            .then_some(ActiveWindowModalOwner::Shortcuts);
        let app_lock_owner = self
            .app_lock_dialog_open
            .then_some(ActiveWindowModalOwner::AppLockDialog);
        let mermaid_owner = self
            .mermaid_zoom_open
            .then_some(ActiveWindowModalOwner::MermaidZoom);

        // Toasts, select popovers, context menus, drag previews, return
        // handoffs, the broadcast menu, and AI floating controls are excluded.
        // They are transient nonblocking layers and keep their own focused
        // input or Escape handling instead of consuming every window key.
        let _native_update_toast_visible = self.native_update_toast_visible;
        [
            new_connection_owner,
            local_shell_owner,
            jump_server_owner,
            host_key_owner,
            keyboard_interactive_owner,
            ai_enable_owner,
            ai_provider_key_remove_owner,
            ai_provider_remove_owner,
            ai_safety_owner,
            ai_summarize_owner,
            ai_owner,
            overlay_owner,
            settings_data_owner,
            remote_shell_owner,
            terminal_trigger_owner,
            cloud_sync_owner,
            tab_owner,
            host_tools_owner,
            native_plugin_owner,
            tab_rename_owner,
            active_tab_owner,
            terminal_cast_owner,
            theme_editor_owner,
            ssh_import_owner,
            command_specs_owner,
            quick_commands_manager_owner,
            ai_text_editor_owner,
            oxide_import_owner,
            oxide_export_owner,
            command_palette_owner,
            version_migration_owner,
            onboarding_owner,
            shortcuts_owner,
            app_lock_owner,
            mermaid_owner,
        ]
        .into_iter()
        .flatten()
        .max_by_key(|owner| owner.render_rank())
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn active_sftp_editor_owns_key(&self, key: &str, cx: &App) -> bool {
        // This mirrors the modal route so later pane-level capture cannot
        // reclaim document keys after the SFTP editor has been selected.
        self.active_window_modal_owner(cx)
            .filter(|owner| {
                matches!(
                    owner,
                    ActiveWindowModalOwner::ActiveTabWindowModal {
                        kind: ActiveTabWindowModalKind::SftpEditor,
                        ..
                    }
                )
            })
            .is_some_and(|owner| !owner.key_route(key).consumes_key())
    }

    pub(in crate::workspace) fn active_window_modal_owner(
        &self,
        cx: &App,
    ) -> Option<ActiveWindowModalOwner> {
        let connection_form = self.connection_form_state(cx);
        let new_connection_open = connection_form.form.is_some();
        let jump_server_open = connection_form
            .form
            .as_ref()
            .is_some_and(|form| form.jump_server_form.is_some());
        let (ai_enable_phase, ai_provider_key_remove_phase, ai_provider_remove_phase, ai_confirm) = {
            let ai = self.ai_entity.read(cx);
            let phase = ai.settings_confirm_phase();
            (
                ai.settings_confirm_is_enable().then_some(phase),
                ai.settings_confirm_is_provider_key_remove()
                    .then_some(phase),
                ai.settings_confirm_provider_name()
                    .is_some()
                    .then_some(phase),
                ai.chat_confirm_owner_snapshot(),
            )
        };
        let (settings_data_directory_phase, theme_editor_phase, settings_ssh_import_phase) = {
            let settings = self.settings_workspace.read(cx);
            (
                settings
                    .data_directory_confirm()
                    .is_some()
                    .then_some(settings.data_directory_confirm_phase()),
                settings
                    .theme_editor_open()
                    .then_some(settings.theme_editor_phase()),
                settings
                    .ssh_config_import_dialog_open()
                    .then_some(settings.ssh_config_import_dialog_phase()),
            )
        };
        let cloud_sync_phase = {
            let cloud_sync = self.cloud_sync.read(cx);
            cloud_sync
                .view
                .confirm
                .is_some()
                .then_some(cloud_sync.view.confirm_presence.phase())
        };
        let native_plugin_phase = {
            let plugins = self.plugin_entity.read(cx);
            plugins
                .confirm_dialog()
                .is_some()
                .then_some(plugins.confirm_phase())
        };
        let (oxide_import_phase, oxide_export_phase) = {
            let session_manager = self.session_manager.read(cx);
            (
                session_manager
                    .oxide_import_dialog
                    .as_ref()
                    .map(|dialog| dialog.presence.phase()),
                session_manager
                    .oxide_export_dialog
                    .as_ref()
                    .map(|dialog| dialog.presence.phase()),
            )
        };
        ActiveWindowModalProjection {
            new_connection_open,
            local_shell_launcher_open: self.local_shell_launcher_open,
            jump_server_open,
            host_key_challenge_open: self.connection_flow.read(cx).has_host_key_challenge(),
            keyboard_interactive_challenge_open: self
                .connection_flow
                .read(cx)
                .has_keyboard_interactive_challenge(),
            ai_enable_phase,
            ai_provider_key_remove_phase,
            ai_provider_remove_phase,
            ai_safety_phase: self
                .ai_entity
                .read(cx)
                .chat_ui()
                .safety_confirm_open
                .then_some(
                    self.ai_entity
                        .read(cx)
                        .chat_ui()
                        .safety_confirm_presence
                        .phase(),
                ),
            ai_summarize_phase: self
                .ai_entity
                .read(cx)
                .chat_ui()
                .summarize_confirm_open
                .then_some(
                    self.ai_entity
                        .read(cx)
                        .chat_ui()
                        .summarize_confirm_presence
                        .phase(),
                ),
            ai_confirm,
            overlay_confirm: self.overlay.read(cx).confirm_owner_snapshot(),
            settings_data_directory_phase,
            remote_shell_integration_open: self
                .workspace_runtime
                .read(cx)
                .remote_shell_integration_confirm_open(),
            terminal_trigger_quick_command_open: self.terminal_trigger_quick_command_pending(),
            cloud_sync_phase,
            tab_close_phase: self.tab_host.read(cx).close_confirm_phase(),
            host_tools_modal: self.host_tools.read(cx).window_modal_snapshot(),
            native_plugin_phase,
            tab_rename_open: self.tab_rename_dialog.is_some(),
            active_tab_modal: self.active_tab_window_modal_owner(cx),
            terminal_cast_player_open: self.terminal.read(cx).cast_player_open(),
            theme_editor_phase,
            settings_ssh_import_phase,
            terminal_command_specs_editor_open: self.terminal_command_specs_editor_open,
            quick_commands_manager_open: self.terminal.read(cx).quick_commands.manager_open(),
            ai_text_editor_open: self.ai_text_editor_dialog.is_some(),
            oxide_import_phase,
            oxide_export_phase,
            command_palette_open: self.command_palette.read(cx).is_open(),
            version_migration_open: self.version_migration.open,
            onboarding_open: self.onboarding.open,
            shortcuts_open: self.shortcuts_modal.open,
            app_lock_dialog_open: self.app_lock.dialog.is_some(),
            mermaid_zoom_open: self.mermaid_zoom.is_some(),
            native_update_toast_visible: self.native_update_notification_open,
        }
        .top_owner()
    }

    fn active_tab_window_modal_owner(&self, cx: &App) -> Option<ActiveTabWindowModalSnapshot> {
        let visible = oxideterm_gpui_ui::motion::ExitPhase::Visible;
        if !self.sidebar_collapsed
            && self.effective_sidebar_panel_section() == SidebarSection::Sessions
            && self.embedded_sftp_node_id.is_some()
            && self.sftp_view.read(cx).current_surface_id == Some(sftp::SftpSurfaceId::Sidebar)
            && let Some(dialog) = self.sftp_view.read(cx).dialog()
        {
            return Some(ActiveTabWindowModalSnapshot {
                kind: if matches!(dialog, crate::workspace::sftp::SftpDialog::Editor { .. }) {
                    ActiveTabWindowModalKind::SftpEditor
                } else {
                    ActiveTabWindowModalKind::SftpDialog
                },
                phase: self.sftp_view.read(cx).dialog_phase(),
            });
        }
        let active_tab = self.active_tab(cx)?;
        match active_tab.kind {
            TabKind::Settings => {
                let settings = self.settings_workspace.read(cx);
                let ai = self.ai_entity.read(cx);
                if settings.portable_password_dialog_open() {
                    Some(ActiveTabWindowModalSnapshot {
                        kind: ActiveTabWindowModalKind::PortablePassword,
                        phase: settings.portable_password_dialog_phase(),
                    })
                } else if settings.managed_key_dialog_open() {
                    Some(ActiveTabWindowModalSnapshot {
                        kind: ActiveTabWindowModalKind::ManagedKey,
                        phase: settings.managed_key_dialog_phase(),
                    })
                } else if let Some(snapshot) = settings.keybinding_reset_confirm_snapshot() {
                    Some(ActiveTabWindowModalSnapshot {
                        kind: ActiveTabWindowModalKind::KeybindingReset,
                        phase: snapshot.phase,
                    })
                } else if ai.knowledge_delete_confirm().is_some() {
                    Some(ActiveTabWindowModalSnapshot {
                        kind: ActiveTabWindowModalKind::KnowledgeDelete,
                        phase: visible,
                    })
                } else if ai.knowledge_document_dialog_open() {
                    Some(ActiveTabWindowModalSnapshot {
                        kind: ActiveTabWindowModalKind::KnowledgeDocumentCreate,
                        phase: ai.knowledge_document_dialog_phase(),
                    })
                } else if ai.knowledge_create_dialog_open() {
                    Some(ActiveTabWindowModalSnapshot {
                        kind: ActiveTabWindowModalKind::KnowledgeCollectionCreate,
                        phase: ai.knowledge_create_dialog_phase(),
                    })
                } else if ai.mcp_dialog_is_open() {
                    Some(ActiveTabWindowModalSnapshot {
                        kind: ActiveTabWindowModalKind::AiMcpServer,
                        phase: ai.mcp_dialog_presence().phase(),
                    })
                } else if settings.navigation_editor_open() {
                    Some(ActiveTabWindowModalSnapshot {
                        kind: ActiveTabWindowModalKind::SettingsNavigationEditor,
                        phase: visible,
                    })
                } else {
                    None
                }
            }
            TabKind::SessionManager => {
                let session_manager = self.session_manager.read(cx);
                if session_manager.delete_confirm.is_some() {
                    Some(ActiveTabWindowModalSnapshot {
                        kind: ActiveTabWindowModalKind::SessionManagerDelete,
                        phase: visible,
                    })
                } else if session_manager.show_group_manager {
                    Some(ActiveTabWindowModalSnapshot {
                        kind: ActiveTabWindowModalKind::SessionManagerGroupManager,
                        phase: visible,
                    })
                } else {
                    None
                }
            }
            TabKind::Forwards => {
                let forwarding = self.forwarding.read(cx);
                if forwarding.delete_confirm_open() {
                    Some(ActiveTabWindowModalSnapshot {
                        kind: ActiveTabWindowModalKind::ForwardDelete,
                        phase: visible,
                    })
                } else if forwarding.edit_form_open() {
                    Some(ActiveTabWindowModalSnapshot {
                        kind: ActiveTabWindowModalKind::ForwardEdit,
                        phase: forwarding.edit_form_phase(),
                    })
                } else {
                    None
                }
            }
            TabKind::Sftp => {
                let sftp = self.sftp_view.read(cx);
                sftp.dialog_is_open().then(|| ActiveTabWindowModalSnapshot {
                    kind: if matches!(
                        sftp.dialog(),
                        Some(crate::workspace::sftp::SftpDialog::Editor { .. })
                    ) {
                        ActiveTabWindowModalKind::SftpEditor
                    } else {
                        ActiveTabWindowModalKind::SftpDialog
                    },
                    phase: sftp.dialog_phase(),
                })
            }
            TabKind::FileManager => self.file_manager.read(cx).dialog.is_some().then_some(
                ActiveTabWindowModalSnapshot {
                    kind: ActiveTabWindowModalKind::FileManagerDialog,
                    phase: visible,
                },
            ),
            _ => None,
        }
    }

    /// Routes the key only to the top rendered modal and always consumes it so
    /// background inputs, terminals, and shortcuts cannot observe it.
    pub(in crate::workspace) fn capture_active_window_modal_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(owner) = self.active_window_modal_owner(cx) else {
            return false;
        };
        let route = owner.key_route(event.keystroke.key.as_str());
        if route.dispatch_owner.is_some() && owner.allows_modal_ime() {
            if self.defer_active_ime_key(&event.keystroke, window, cx) {
                // The root capture handler immediately repeats this check and
                // yields to the platform pipeline for the top modal input.
                return false;
            }
            if self.handle_active_text_input_edit_shortcut(&event.keystroke, cx)
                || self.handle_active_text_input_delete_selection(&event.keystroke, cx)
                || self.handle_active_text_input_newline(&event.keystroke, cx)
                || self.handle_active_text_input_transpose(&event.keystroke, cx)
                || self.handle_active_text_input_navigation(&event.keystroke, cx)
            {
                // Editable controls inside the top modal own mutation and
                // navigation keys before modal-level Escape/Enter handling.
                return true;
            }
        }
        let Some(owner) = route.dispatch_owner else {
            return route.consumes_key();
        };
        match owner {
            ActiveWindowModalOwner::NewConnection | ActiveWindowModalOwner::JumpServer => {
                let _ = self.handle_new_connection_key(event, window, cx);
            }
            ActiveWindowModalOwner::LocalShellLauncher => {
                let _ = self.handle_local_shell_launcher_key(event, window, cx);
            }
            ActiveWindowModalOwner::HostKeyChallenge => {
                if event.keystroke.key.as_str() == "escape" {
                    self.cancel_host_key_challenge(cx);
                }
            }
            ActiveWindowModalOwner::KeyboardInteractiveChallenge => {
                let _ = self.handle_keyboard_interactive_key(event, window, cx);
            }
            ActiveWindowModalOwner::AiEnable { .. }
            | ActiveWindowModalOwner::AiProviderKeyRemove { .. }
            | ActiveWindowModalOwner::AiProviderRemove { .. } => {
                let _ = self.handle_ai_settings_confirm_key(event, cx);
            }
            ActiveWindowModalOwner::AiSafety { .. } => {
                let _ = self.handle_ai_safety_confirm_key(event, cx);
            }
            ActiveWindowModalOwner::AiSummarize { .. } => {
                let _ = self.handle_ai_summarize_confirm_key(event, cx);
            }
            ActiveWindowModalOwner::AiClearAll { .. }
            | ActiveWindowModalOwner::AiDeleteMessage { .. } => {
                let _ = self.handle_ai_chat_confirm_key(event, cx);
            }
            ActiveWindowModalOwner::SettingsReset { .. } => {
                let _ = self.handle_settings_reset_confirm_key(event, cx);
            }
            ActiveWindowModalOwner::SettingsDataDirectory { .. } => {
                let _ = self.handle_settings_data_directory_confirm_key(event, cx);
            }
            ActiveWindowModalOwner::RemoteShellIntegration => {
                let _ = self.handle_remote_shell_integration_confirm_key(event, cx);
            }
            ActiveWindowModalOwner::TerminalTriggerQuickCommand => {
                let _ = self.handle_terminal_trigger_quick_command_key(event, cx);
            }
            ActiveWindowModalOwner::CloudSync { .. } => {
                let _ = self.handle_cloud_sync_confirm_key(event, cx);
            }
            ActiveWindowModalOwner::NodeDisconnect { .. } => {
                let _ = self.handle_node_disconnect_confirm_key(event, window, cx);
            }
            ActiveWindowModalOwner::TabClose { .. } => {
                let _ = self.handle_tab_close_confirm_key(event, window, cx);
            }
            ActiveWindowModalOwner::HostProcessConfirm { phase } => {
                let _ = self.handle_host_tools_window_modal_key(
                    connection_monitor::HostToolsWindowModalSnapshot::ProcessConfirm(phase),
                    event,
                    cx,
                );
            }
            ActiveWindowModalOwner::HostDockerConfirm { phase } => {
                let _ = self.handle_host_tools_window_modal_key(
                    connection_monitor::HostToolsWindowModalSnapshot::DockerConfirm(phase),
                    event,
                    cx,
                );
            }
            ActiveWindowModalOwner::HostDockerLogs => {
                let _ = self.handle_host_tools_window_modal_key(
                    connection_monitor::HostToolsWindowModalSnapshot::DockerLogs,
                    event,
                    cx,
                );
            }
            ActiveWindowModalOwner::HostServiceConfirm { phase } => {
                let _ = self.handle_host_tools_window_modal_key(
                    connection_monitor::HostToolsWindowModalSnapshot::ServiceConfirm(phase),
                    event,
                    cx,
                );
            }
            ActiveWindowModalOwner::HostServiceLogs => {
                let _ = self.handle_host_tools_window_modal_key(
                    connection_monitor::HostToolsWindowModalSnapshot::ServiceLogs,
                    event,
                    cx,
                );
            }
            ActiveWindowModalOwner::HostTmuxConfirm { phase } => {
                let _ = self.handle_host_tools_window_modal_key(
                    connection_monitor::HostToolsWindowModalSnapshot::TmuxConfirm(phase),
                    event,
                    cx,
                );
            }
            ActiveWindowModalOwner::HostTmuxInput => {
                let _ = self.handle_host_tools_window_modal_key(
                    connection_monitor::HostToolsWindowModalSnapshot::TmuxInput,
                    event,
                    cx,
                );
            }
            ActiveWindowModalOwner::HostScheduleConfirm { phase } => {
                let _ = self.handle_host_tools_window_modal_key(
                    connection_monitor::HostToolsWindowModalSnapshot::ScheduleConfirm(phase),
                    event,
                    cx,
                );
            }
            ActiveWindowModalOwner::HostScheduleLogs => {
                let _ = self.handle_host_tools_window_modal_key(
                    connection_monitor::HostToolsWindowModalSnapshot::ScheduleLogs,
                    event,
                    cx,
                );
            }
            ActiveWindowModalOwner::NativePluginConfirm { .. } => {
                let _ = self.handle_native_plugin_confirm_key(event, cx);
            }
            ActiveWindowModalOwner::TabRename => {
                let _ = self.handle_tab_rename_dialog_key(event, window, cx);
            }
            ActiveWindowModalOwner::ActiveTabWindowModal { kind, .. } => {
                let _ = self.handle_active_tab_window_modal_key(kind, event, window, cx);
            }
            ActiveWindowModalOwner::TerminalCastPlayer => {
                if self.terminal.read(cx).cast_search_focused() {
                    self.handle_terminal_cast_search_key(event, cx);
                } else if event.keystroke.key.as_str() == "escape" {
                    self.close_terminal_cast_player(cx);
                }
            }
            ActiveWindowModalOwner::ThemeEditor { .. } => {
                if event.keystroke.key.as_str() == "escape" {
                    self.close_theme_editor(cx);
                }
            }
            ActiveWindowModalOwner::SettingsSshConfigImport { .. } => {
                if event.keystroke.key.as_str() == "escape" {
                    self.close_settings_ssh_config_import_dialog(cx);
                }
            }
            ActiveWindowModalOwner::TerminalCommandSpecsEditor => {
                if event.keystroke.key.as_str() == "escape" {
                    self.close_terminal_command_specs_editor(cx);
                }
            }
            ActiveWindowModalOwner::QuickCommandsManager => {
                if self
                    .terminal
                    .read(cx)
                    .quick_commands
                    .focused_input()
                    .is_some()
                {
                    self.handle_quick_commands_key(event, cx);
                } else if event.keystroke.key.as_str() == "escape" {
                    self.close_quick_commands_manager(cx);
                }
            }
            ActiveWindowModalOwner::AiTextEditor => {
                if event.keystroke.key.as_str() == "escape" {
                    self.close_ai_text_editor(false, cx);
                }
            }
            ActiveWindowModalOwner::OxideImport { .. } => {
                let _ = self.handle_oxide_import_modal_key(event, cx);
            }
            ActiveWindowModalOwner::OxideExport { .. } => {
                let _ = self.handle_oxide_export_modal_key(event, cx);
            }
            ActiveWindowModalOwner::CommandPalette => {
                self.handle_command_palette_key(event, window, cx);
            }
            ActiveWindowModalOwner::VersionMigration => {
                let _ = self.handle_version_migration_key(event, cx);
            }
            ActiveWindowModalOwner::Onboarding => {
                let _ = self.handle_onboarding_key(event, cx);
            }
            ActiveWindowModalOwner::LegalNotice { .. } => {
                let _ = self.handle_help_legal_notice_key(event, cx);
            }
            ActiveWindowModalOwner::NativeUpdateReleaseNotes { .. } => {
                let _ = self.handle_native_update_release_notes_key(event, cx);
            }
            ActiveWindowModalOwner::Shortcuts => {
                self.handle_shortcuts_modal_key(event, cx);
            }
            ActiveWindowModalOwner::AppLockDialog => {
                let _ = self.handle_app_lock_dialog_key(event, cx);
            }
            ActiveWindowModalOwner::MermaidZoom => {
                if event.keystroke.key.as_str() == "escape" {
                    self.mermaid_zoom = None;
                    cx.notify();
                }
            }
        }
        route.consumes_key()
    }

    fn handle_active_tab_window_modal_key(
        &mut self,
        kind: ActiveTabWindowModalKind,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match kind {
            ActiveTabWindowModalKind::PortablePassword => {
                if event.keystroke.key.as_str() == "escape" {
                    self.close_portable_password_change_dialog(cx);
                }
                true
            }
            ActiveTabWindowModalKind::ManagedKey => {
                if event.keystroke.key.as_str() == "escape" {
                    self.close_managed_key_dialog(cx);
                }
                true
            }
            ActiveTabWindowModalKind::KeybindingReset => {
                self.handle_keybinding_reset_confirm_key(event, window, cx)
            }
            ActiveTabWindowModalKind::KnowledgeDelete => {
                self.handle_knowledge_delete_confirm_key(event, cx)
            }
            ActiveTabWindowModalKind::KnowledgeDocumentCreate => {
                self.handle_knowledge_document_dialog_key(event, cx)
            }
            ActiveTabWindowModalKind::KnowledgeCollectionCreate => {
                self.handle_knowledge_collection_dialog_key(event, cx)
            }
            ActiveTabWindowModalKind::AiMcpServer => self.handle_ai_mcp_add_dialog_key(event, cx),
            ActiveTabWindowModalKind::SettingsNavigationEditor => {
                if event.keystroke.key.as_str() == "escape" {
                    self.close_settings_navigation_editor(cx);
                }
                true
            }
            ActiveTabWindowModalKind::SessionManagerGroupManager => {
                if self.handle_session_manager_basic_dialog_footer_key(event, cx) {
                    return true;
                }
                if event.keystroke.key.as_str() == "escape" {
                    self.close_session_group_manager(cx);
                }
                true
            }
            ActiveTabWindowModalKind::SessionManagerDelete => {
                self.handle_session_manager_delete_confirm_key(event, cx)
            }
            ActiveTabWindowModalKind::ForwardEdit => self.handle_forward_edit_modal_key(event, cx),
            ActiveTabWindowModalKind::ForwardDelete => {
                self.handle_forward_delete_confirm_key(event, cx)
            }
            ActiveTabWindowModalKind::SftpEditor | ActiveTabWindowModalKind::SftpDialog => {
                self.handle_sftp_key(event, window, cx)
            }
            ActiveTabWindowModalKind::FileManagerDialog => self.handle_file_manager_key(event, cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VISIBLE: oxideterm_gpui_ui::motion::ExitPhase =
        oxideterm_gpui_ui::motion::ExitPhase::Visible;
    const EXITING: oxideterm_gpui_ui::motion::ExitPhase =
        oxideterm_gpui_ui::motion::ExitPhase::Exiting;

    fn ai_clear_all() -> ai_state::AiChatConfirmOwnerSnapshot {
        ai_state::AiChatConfirmOwnerSnapshot {
            kind: ai_state::AiChatConfirmOwnerKind::ClearAll,
            phase: VISIBLE,
        }
    }

    fn settings_reset() -> overlay::WorkspaceOverlayConfirmOwnerSnapshot {
        overlay::WorkspaceOverlayConfirmOwnerSnapshot {
            kind: overlay::WorkspaceOverlayConfirmOwnerKind::SettingsReset,
            phase: VISIBLE,
        }
    }

    #[test]
    fn later_blocking_portals_win_across_the_complete_root_stack() {
        let projection = ActiveWindowModalProjection {
            new_connection_open: true,
            keyboard_interactive_challenge_open: true,
            ai_summarize_phase: Some(VISIBLE),
            settings_data_directory_phase: Some(VISIBLE),
            host_tools_modal: Some(connection_monitor::HostToolsWindowModalSnapshot::ScheduleLogs),
            native_plugin_phase: Some(VISIBLE),
            active_tab_modal: Some(ActiveTabWindowModalSnapshot {
                kind: ActiveTabWindowModalKind::ForwardDelete,
                phase: VISIBLE,
            }),
            terminal_cast_player_open: true,
            theme_editor_phase: Some(VISIBLE),
            settings_ssh_import_phase: Some(VISIBLE),
            oxide_export_phase: Some(VISIBLE),
            command_palette_open: true,
            ..Default::default()
        };
        assert_eq!(
            projection.top_owner(),
            Some(ActiveWindowModalOwner::CommandPalette)
        );
    }

    #[test]
    fn exiting_top_owner_remains_the_only_owner_until_unmounted() {
        let projection = ActiveWindowModalProjection {
            ai_confirm: Some(ai_clear_all()),
            tab_close_phase: Some(EXITING),
            ..Default::default()
        };
        let owner = projection.top_owner().expect("top modal owner");
        assert_eq!(owner, ActiveWindowModalOwner::TabClose { phase: EXITING });
        let route = owner.key_route("enter");
        assert!(route.consumes_key());
        assert_eq!(route.dispatch_owner, None);
    }

    #[test]
    fn exiting_active_tab_modal_blocks_lower_confirm_without_dispatching() {
        let projection = ActiveWindowModalProjection {
            overlay_confirm: Some(settings_reset()),
            active_tab_modal: Some(ActiveTabWindowModalSnapshot {
                kind: ActiveTabWindowModalKind::ForwardEdit,
                phase: EXITING,
            }),
            ..Default::default()
        };
        let owner = projection.top_owner().expect("active tab modal owner");
        assert_eq!(
            owner,
            ActiveWindowModalOwner::ActiveTabWindowModal {
                kind: ActiveTabWindowModalKind::ForwardEdit,
                phase: EXITING,
            }
        );
        let route = owner.key_route("escape");
        assert!(route.consumes_key());
        assert_eq!(route.dispatch_owner, None);
    }

    #[test]
    fn ai_text_editor_yields_document_keys_but_captures_escape() {
        let owner = ActiveWindowModalOwner::AiTextEditor;

        for key in ["enter", "backspace", "left", "tab", "x"] {
            let route = owner.key_route(key);
            assert!(!route.consumes_key(), "{key} must reach the focused editor");
            assert_eq!(route.dispatch_owner, None);
        }

        let escape = owner.key_route("escape");
        assert!(escape.consumes_key());
        assert_eq!(escape.dispatch_owner, Some(owner));
    }

    #[test]
    fn sftp_editor_yields_document_keys_only_while_visible() {
        let visible_owner = ActiveWindowModalOwner::ActiveTabWindowModal {
            kind: ActiveTabWindowModalKind::SftpEditor,
            phase: VISIBLE,
        };

        for key in ["enter", "backspace", "left", "tab", "x"] {
            let route = visible_owner.key_route(key);
            assert!(!route.consumes_key(), "{key} must reach the focused editor");
            assert_eq!(route.dispatch_owner, None);
        }
        let escape = visible_owner.key_route("escape");
        assert!(escape.consumes_key());
        assert_eq!(escape.dispatch_owner, Some(visible_owner));

        let exiting_owner = ActiveWindowModalOwner::ActiveTabWindowModal {
            kind: ActiveTabWindowModalKind::SftpEditor,
            phase: EXITING,
        };
        let exiting_key = exiting_owner.key_route("x");
        assert!(exiting_key.consumes_key());
        assert_eq!(exiting_key.dispatch_owner, None);
    }
}
