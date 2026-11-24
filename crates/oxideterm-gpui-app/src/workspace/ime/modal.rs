//! Resolve shared text input against the same blocking owner as key capture.
use super::super::root::modal_owner::{
    ActiveTabWindowModalKind as TabModal, ActiveWindowModalOwner as Owner,
    HostRedisDialogKind as Redis,
};
use super::*;
pub(super) fn host_target(input: HostToolsTextInput) -> WorkspaceImeTarget {
    match input {
        HostToolsTextInput::ProcessSearch => WorkspaceImeTarget::HostProcessSearch,
        HostToolsTextInput::ProcessRenice => WorkspaceImeTarget::HostProcessRenice,
        HostToolsTextInput::DockerSearch => WorkspaceImeTarget::HostDockerSearch,
        HostToolsTextInput::ServiceSearch => WorkspaceImeTarget::HostServiceSearch,
        HostToolsTextInput::LogSearch => WorkspaceImeTarget::HostLogSearch,
        HostToolsTextInput::TmuxSearch => WorkspaceImeTarget::HostTmuxSearch,
        HostToolsTextInput::TmuxDialog => WorkspaceImeTarget::HostTmuxDialogInput,
        HostToolsTextInput::MuxDialog => WorkspaceImeTarget::HostMuxDialog,
        HostToolsTextInput::PortSearch => WorkspaceImeTarget::HostPortSearch,
        HostToolsTextInput::ScheduleSearch => WorkspaceImeTarget::HostScheduleSearch,
        HostToolsTextInput::FilesystemSearch => WorkspaceImeTarget::HostFilesystemSearch,
        HostToolsTextInput::PackageSearch => WorkspaceImeTarget::HostPackageSearch,
        HostToolsTextInput::ContainerSearch => WorkspaceImeTarget::HostContainerSearch,
        HostToolsTextInput::ContainerDialog => WorkspaceImeTarget::HostContainerDialog,
        HostToolsTextInput::ContainerDialogRegistry => {
            WorkspaceImeTarget::HostContainerDialogRegistry
        }
        HostToolsTextInput::ContainerRunName => WorkspaceImeTarget::HostContainerRunName,
        HostToolsTextInput::ContainerRunPorts => WorkspaceImeTarget::HostContainerRunPorts,
        HostToolsTextInput::ContainerRunVolumes => WorkspaceImeTarget::HostContainerRunVolumes,
        HostToolsTextInput::ContainerRunCommand => WorkspaceImeTarget::HostContainerRunCommand,
        HostToolsTextInput::RedisPassword => WorkspaceImeTarget::HostRedisPassword,
        HostToolsTextInput::RedisRename => WorkspaceImeTarget::HostRedisRename,
        HostToolsTextInput::RedisSetValue => WorkspaceImeTarget::HostRedisSetValue,
        HostToolsTextInput::RedisSetValueField => WorkspaceImeTarget::HostRedisSetValueField,
        HostToolsTextInput::RedisCreateKeyName => WorkspaceImeTarget::HostRedisCreateKeyName,
        HostToolsTextInput::RedisCreateKeyValue => WorkspaceImeTarget::HostRedisCreateKeyValue,
        HostToolsTextInput::RedisCreateKeyField => WorkspaceImeTarget::HostRedisCreateKeyField,
        HostToolsTextInput::RedisCreateKeyTtl => WorkspaceImeTarget::HostRedisCreateKeyTtl,
        HostToolsTextInput::RedisTtl => WorkspaceImeTarget::HostRedisTtl,
        HostToolsTextInput::RedisSearch => WorkspaceImeTarget::HostRedisSearch,
    }
}
impl WorkspaceApp {
    pub(super) fn modal_ime_target(&self, owner: Owner, cx: &App) -> Option<WorkspaceImeTarget> {
        use WorkspaceImeTarget as Target;
        if owner.phase() != oxideterm_gpui_ui::motion::ExitPhase::Visible {
            return None;
        }
        let settings = |accepts: fn(SettingsInput) -> bool| {
            [
                self.focused_settings_input,
                self.settings_workspace
                    .read(cx)
                    .settings_entity_focused_input(),
                self.knowledge_page(cx).read(cx).focused_settings_input(),
            ]
            .into_iter()
            .flatten()
            .find(|input| accepts(*input))
            .map(Target::Settings)
        };
        let manager = |accepts: fn(SessionManagerInput) -> bool| {
            self.connections
                .session_manager
                .read(cx)
                .focused_input()
                .filter(|input| accepts(*input))
                .map(Target::SessionManager)
        };
        match owner {
            Owner::CommandPalette => {
                (!self.command_palette.read(cx).is_closing()).then_some(Target::CommandPalette)
            }
            Owner::Shortcuts => (self.shortcuts_modal.presence.phase()
                == oxideterm_gpui_ui::motion::ExitPhase::Visible)
                .then_some(Target::ShortcutsModalSearch),
            Owner::TabRename => Some(Target::TabRename),
            Owner::AppLockDialog => settings(|i| {
                matches!(
                    i,
                    SettingsInput::AppLockCurrentPassword
                        | SettingsInput::AppLockNewPassword
                        | SettingsInput::AppLockConfirmPassword
                )
            }),
            Owner::KeyboardInteractiveChallenge => self
                .connections
                .flow
                .read(cx)
                .focused_keyboard_interactive_prompt()
                .map(Target::KeyboardInteractive),
            Owner::JumpServer | Owner::SshTunnel | Owner::NewConnection => {
                if let Some(form) = self
                    .connection_form_state(cx)
                    .form
                    .as_ref()
                    .filter(|form| form.field_focused)
                {
                    return Some(Target::NewConnection(form.focused_field));
                }
                let ui = self.plugin_ui_state(cx);
                let key = ui.focused_input?;
                let context = ui.context(key)?;
                (context.surface_kind == "connectionForm"
                    && self.native_plugin_ui_control_is_visible(key, cx))
                .then_some(Target::PluginControl {
                    key,
                    secret: context.control_kind == "password",
                })
            }
            Owner::OxideImport { .. } => manager(|i| i == SessionManagerInput::OxideImportPassword),
            Owner::OxideExport { .. } => manager(|i| {
                matches!(
                    i,
                    SessionManagerInput::OxideExportPassword
                        | SessionManagerInput::OxideExportConfirmPassword
                        | SessionManagerInput::OxideExportDescription
                )
            }),
            Owner::BatchAdd { .. } => manager(|i| {
                matches!(
                    i,
                    SessionManagerInput::BatchPassword
                        | SessionManagerInput::BatchKeyPath
                        | SessionManagerInput::BatchTemplateName
                )
            }),
            Owner::QuickCommandsManager => self
                .terminal
                .read(cx)
                .quick_commands
                .focused_input()
                .filter(|i| *i != QuickCommandInput::CommandText)
                .map(Target::QuickCommand),
            Owner::TerminalCastPlayer => self
                .terminal
                .read(cx)
                .cast_search_focused()
                .then_some(Target::TerminalCastSearch),
            Owner::ThemeEditor { .. } => {
                settings(oxideterm_gpui_settings_view::theme_editor::is_theme_editor_input)
            }
            Owner::SettingsSshConfigImport { .. } => {
                settings(|i| i == SettingsInput::ConnectionImportTargetGroup)
            }
            Owner::TerminalCommandSpecsEditor => {
                settings(|i| i == SettingsInput::TerminalCommandSpecsJson)
            }
            Owner::TerminalTriggerEditor => {
                settings(oxideterm_gpui_settings_view::terminal_triggers::is_terminal_trigger_input)
            }
            Owner::SemanticSchemeEditor => {
                settings(super::super::settings::highlight::is_semantic_scheme_input)
            }
            Owner::HighlightRuleEditor => {
                settings(super::super::settings::highlight::is_highlight_rule_input)
            }
            Owner::Onboarding => self
                .settings_workspace
                .read(cx)
                .settings_entity_focused_input()
                .or(self.focused_settings_input)
                .map(Target::Settings),
            Owner::HostContainerInput
            | Owner::HostTmuxInput
            | Owner::HostMuxInput
            | Owner::HostRedisPassword
            | Owner::HostRedisDialog(_) => {
                let t = host_target(self.host_tools.read(cx).ui.focused_input?);
                matches!(
                    (owner, t),
                    (
                        Owner::HostContainerInput,
                        Target::HostContainerDialog
                            | Target::HostContainerDialogRegistry
                            | Target::HostContainerRunName
                            | Target::HostContainerRunPorts
                            | Target::HostContainerRunVolumes
                            | Target::HostContainerRunCommand
                    ) | (Owner::HostTmuxInput, Target::HostTmuxDialogInput)
                        | (Owner::HostMuxInput, Target::HostMuxDialog)
                        | (Owner::HostRedisPassword, Target::HostRedisPassword)
                        | (
                            Owner::HostRedisDialog(Redis::Rename),
                            Target::HostRedisRename
                        )
                        | (Owner::HostRedisDialog(Redis::Ttl), Target::HostRedisTtl)
                        | (
                            Owner::HostRedisDialog(Redis::SetValue),
                            Target::HostRedisSetValue | Target::HostRedisSetValueField
                        )
                        | (
                            Owner::HostRedisDialog(Redis::CreateKey),
                            Target::HostRedisCreateKeyName
                                | Target::HostRedisCreateKeyValue
                                | Target::HostRedisCreateKeyField
                                | Target::HostRedisCreateKeyTtl
                        )
                )
                .then_some(t)
            }
            Owner::ActiveTabWindowModal { kind, .. } => match kind {
                TabModal::KnowledgeRename => Some(Target::KnowledgeRename),
                TabModal::KnowledgeCollectionCreate => {
                    settings(|i| i == SettingsInput::KnowledgeCollectionName)
                }
                TabModal::KnowledgeDocumentCreate => {
                    settings(|i| i == SettingsInput::KnowledgeDocumentTitle)
                }
                TabModal::PortablePassword => settings(|i| {
                    matches!(
                        i,
                        SettingsInput::PortableCurrentPassword
                            | SettingsInput::PortableNewPassword
                            | SettingsInput::PortableConfirmPassword
                    )
                }),
                TabModal::ManagedKey => {
                    settings(oxideterm_gpui_settings_view::components::is_connection_key_input)
                }
                TabModal::AiMcpServer => settings(|i| {
                    matches!(
                        i,
                        SettingsInput::AiMcpName
                            | SettingsInput::AiMcpCommand
                            | SettingsInput::AiMcpArgs
                            | SettingsInput::AiMcpUrl
                            | SettingsInput::AiMcpAuthHeaderName
                            | SettingsInput::AiMcpAuthToken
                            | SettingsInput::AiMcpEnvKey(_)
                            | SettingsInput::AiMcpEnvValue(_)
                            | SettingsInput::AiMcpHeaderKey(_)
                            | SettingsInput::AiMcpHeaderValue(_)
                    )
                }),
                TabModal::FileManagerDialog => self
                    .file_manager
                    .read(cx)
                    .focused_input()
                    .filter(|i| *i == FileManagerInput::DialogValue)
                    .map(Target::FileManager),
                TabModal::SessionManagerGroupManager => {
                    manager(|i| i == SessionManagerInput::GroupName)
                }
                TabModal::ForwardEdit => {
                    let page = self.active_content_tab_id(cx)?;
                    self.forwarding
                        .read(cx)
                        .page_for(page, cx)?
                        .focused_input()
                        .filter(|i| {
                            matches!(
                                i,
                                ForwardInput::EditBindAddress
                                    | ForwardInput::EditBindPort
                                    | ForwardInput::EditTargetHost
                                    | ForwardInput::EditTargetPort
                            )
                        })
                        .map(|i| Target::Forwards(page, i))
                }
                TabModal::SftpDialog => {
                    let surface = if self.embedded_sftp_sidebar_dialog_view(cx).is_some() {
                        super::super::sftp::SftpSurfaceId::Sidebar
                    } else {
                        super::super::sftp::SftpSurfaceId::Tab(self.active_content_tab_id(cx)?)
                    };
                    let _scope = self.enter_sftp_surface(surface);
                    self.sftp_view()
                        .read(cx)
                        .focused_input()
                        .filter(|i| *i == SftpInput::DialogValue)
                        .map(|i| Target::Sftp(surface, i))
                }
                _ => None,
            },
            _ => None,
        }
    }
}
