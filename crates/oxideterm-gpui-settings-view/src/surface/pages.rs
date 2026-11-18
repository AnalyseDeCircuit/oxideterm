use std::{path::Path, rc::Rc};

use gpui::{AnyElement, App, Entity, Pixels, Point, ScrollHandle, Window, div, prelude::*};
use oxideterm_connections::ConnectionStore;
use oxideterm_gpui_ui::ConfirmDialogAction;
use oxideterm_settings_model::SettingsSectionId;
use oxideterm_terminal::ShellInfo;

use crate::{
    ai::{AiSkillAction, AiSkillData, skills::AiSkillsPage},
    app_lock_page::{AppLockDialog, render_app_lock_settings},
    appearance_page::{AppearanceAction, AppearancePageData},
    background_page::{BackgroundPage, BackgroundRequest},
    components::SettingsControls,
    connections_page::{ConnectionPage, ConnectionRequest},
    credentials_page::{CredentialPage, CredentialRequest},
    entity::SettingsWorkspaceEntity,
    general_page::{GeneralPage, GeneralRequest, render_focus_handoff_commands},
    help_page::{HelpPage, HelpRequest},
    highlight_page::{HighlightPage, HighlightRequest},
    keybinding_page::{KeybindingPage, KeybindingRequest, KeybindingTextRole},
    knowledge_page::{KnowledgeSettingsFrame, KnowledgeSettingsPage, KnowledgeSettingsRequest},
    local_terminal::{LocalTerminalPage, LocalTerminalRequest},
    mcp_page::{McpSettingsFrame, McpSettingsPage, McpSettingsRequest},
    portable_runtime::{PortableSettingsPage, PortableSettingsRequest},
    proxy_page::{ProxyPage, ProxyRequest},
    remote_shell::{RemoteShellIntegrationCardSnapshot, RemoteShellPage, RemoteShellRequest},
    terminal_page::TerminalSettingsSection,
    terminal_triggers::{TerminalTriggerPage, TerminalTriggerRequest},
};

pub enum PreparedSettingsPage {
    Knowledge(crate::knowledge_page::KnowledgeSection),
    ThemePreview(AnyElement),
    Background {
        settings: Box<oxideterm_settings::PersistedSettings>,
        preview: Option<AnyElement>,
        animated: bool,
    },
}

pub struct KnowledgePageData<'a> {
    pub frame: KnowledgeSettingsFrame<'a>,
    pub provider_has_key: Rc<dyn Fn(&str, &App) -> bool>,
    pub selected: Rc<dyn Fn(&str, &App) -> bool>,
    pub cut: Rc<dyn Fn(&str, &App) -> bool>,
    pub chevron: Rc<dyn Fn(bool, &App) -> AnyElement>,
}

pub struct TriggerPageRuntime<'a> {
    pub store: &'a ConnectionStore,
    pub pane_exists: Rc<dyn Fn(oxideterm_workspace::PaneId, &App) -> bool>,
    pub applies: Rc<
        dyn Fn(
            oxideterm_workspace::PaneId,
            &oxideterm_terminal_triggers::TerminalTrigger,
            &App,
        ) -> bool,
    >,
    pub enabled: Rc<dyn Fn(oxideterm_workspace::PaneId, &str, &App) -> bool>,
    pub command_label: Rc<dyn Fn(&str, &App) -> Option<String>>,
}

pub enum SettingsPageData<'a> {
    AiBasic,
    AiContext,
    AiContextWindows(crate::components::SettingsControlsSource),
    AiProviders(Entity<crate::ai::providers::AiProviderSettingsView>),
    AiMcp(Entity<crate::ai::mcp::AiMcpSettingsView>),
    AiSubagents,
    AiSkills(AiSkillData),
    Connections(&'a ConnectionStore, Option<ConfirmDialogAction>),
    Credentials(&'a ConnectionStore),
    KnownHosts,
    AppLock(bool, bool, Option<String>),
    Proxy,
    Mcp(
        McpSettingsFrame<'a>,
        Rc<dyn Fn(&oxideterm_public_mcp::ClientRef, &str, &App) -> String>,
    ),
    General {
        data_dir: Option<oxideterm_settings::DataDirectoryInfo>,
        history_failed: bool,
        command_specs: (String, String),
    },
    Appearance(
        AppearancePageData,
        Option<PreparedSettingsPage>,
        Rc<dyn Fn(&Path) -> bool>,
    ),
    LocalTerminal {
        shells: Vec<ShellInfo>,
        focused_input: Option<(crate::SettingsInput, &'a str)>,
        credential_count: usize,
    },
    Help(String),
    Portable(&'a Path),
    RemoteShell(Option<String>, RemoteShellIntegrationCardSnapshot),
    Triggers(TriggerPageRuntime<'a>),
    Highlight,
    Keybindings(Vec<crate::keybindings::ActionDefinition>),
    Knowledge(
        crate::knowledge_page::KnowledgeSection,
        KnowledgePageData<'a>,
    ),
    Sftp,
    Ide,
    Terminal(TerminalSettingsSection),
}

pub enum SettingsPageAction<'a> {
    AiSkills(AiSkillAction),
    Connection(ConnectionRequest),
    Credential(CredentialRequest),
    OpenKnownHostsFile(&'a Path),
    Proxy(ProxyRequest),
    Mcp(McpSettingsRequest),
    Help(HelpRequest),
    Portable(PortableSettingsRequest),
    RemoteShell(RemoteShellRequest),
    Appearance(AppearanceAction),
    EditBackground(Box<dyn FnOnce(&mut oxideterm_settings::BackgroundStyle)>),
    Keybindings(KeybindingRequest),
    ToggleFocusHandoff(String),
}

pub enum SettingsPageWindowAction {
    General(GeneralRequest),
    Highlight(HighlightRequest),
    LocalTerminal(LocalTerminalRequest),
    Trigger(TerminalTriggerRequest),
    AppLock(AppLockDialog),
    Background(BackgroundRequest),
    Knowledge(KnowledgeSettingsRequest, Point<Pixels>),
}

pub struct SettingsPageFrame<'a> {
    pub controls: SettingsControls<'a>,
    pub data: SettingsPageData<'a>,
    pub scroll: Rc<dyn Fn(&str, &App) -> ScrollHandle>,
    pub text: Rc<dyn Fn(KeybindingTextRole, u64, String, u32) -> AnyElement>,
    pub action: Rc<dyn Fn(SettingsPageAction<'_>, &mut App)>,
    pub window_action: Rc<dyn Fn(SettingsPageWindowAction, &mut Window, &mut App)>,
}

#[derive(Clone, Copy)]
pub(crate) enum SettingsDialog {
    ManagedKeys,
    SshImport,
    Highlight,
    Trigger,
    McpClient,
}

impl SettingsDialog {
    pub(crate) fn section(self) -> SettingsSectionId {
        match self {
            Self::ManagedKeys => SettingsSectionId::ConnectionsSshKeys,
            Self::SshImport => SettingsSectionId::ConnectionsSshConfigImport,
            Self::Highlight => SettingsSectionId::TerminalHighlightRules,
            Self::Trigger => SettingsSectionId::TerminalAwarenessTriggers,
            Self::McpClient => SettingsSectionId::NetworkPublicMcp,
        }
    }
}

impl SettingsPageFrame<'_> {
    pub fn render(
        self,
        state: &Entity<SettingsWorkspaceEntity>,
        section: SettingsSectionId,
        cx: &App,
    ) -> AnyElement {
        self.render_target(state, section, None, cx)
            .unwrap_or_else(|| div().into_any_element())
    }

    pub(crate) fn render_dialog(
        self,
        state: &Entity<SettingsWorkspaceEntity>,
        dialog: SettingsDialog,
        cx: &App,
    ) -> Option<AnyElement> {
        self.render_target(state, dialog.section(), Some(dialog), cx)
    }

    fn render_target(
        self,
        state: &Entity<SettingsWorkspaceEntity>,
        section: SettingsSectionId,
        dialog: Option<SettingsDialog>,
        cx: &App,
    ) -> Option<AnyElement> {
        let Self {
            controls,
            data,
            scroll,
            text,
            action,
            window_action,
        } = self;
        let scroll = |key: &str| scroll(key, cx);
        Some(match data {
            SettingsPageData::AiBasic => crate::ai::basic::basic_section(&controls, section, cx),
            SettingsPageData::AiProviders(view) => view.into_any_element(),
            SettingsPageData::AiMcp(view) => view.into_any_element(),
            SettingsPageData::AiSubagents => crate::ai::subagents::render(&controls, state, cx),
            SettingsPageData::AiContext => {
                crate::ai::context::AiContextPage { controls, state }.render(section, cx)
            }
            SettingsPageData::AiContextWindows(source) => {
                crate::ai::context_windows::render(&controls, state, source, cx)
            }
            SettingsPageData::AiSkills(data) => {
                let enabled = controls.settings.ai.enabled;
                let page = AiSkillsPage {
                    controls,
                    state,
                    data,
                    dispatch: Rc::new(move |request, cx| action(SettingsPageAction::AiSkills(request), cx)),
                };
                crate::ai::available(page.render(cx), enabled)
            }
            SettingsPageData::Connections(store, footer_focus) => {
                let page = ConnectionPage {
                    controls,
                    state,
                    store,
                    dispatch: Rc::new(move |request, cx| {
                        action(SettingsPageAction::Connection(request), cx)
                    }),
                    scroll: &scroll,
                    footer_focus,
                };
                match dialog {
                    Some(SettingsDialog::ManagedKeys) => {
                        return page.render_settings_managed_key_dialog(cx);
                    }
                    Some(SettingsDialog::SshImport) => {
                        return page.render_settings_ssh_config_import_dialog(cx);
                    }
                    _ => page.section(section, cx),
                }
            }
            SettingsPageData::Credentials(store) => CredentialPage {
                controls,
                state,
                store,
                scroll: &scroll,
                dispatch: Rc::new(move |request, cx| {
                    action(SettingsPageAction::Credential(request), cx)
                }),
            }
            .section(section, cx),
            SettingsPageData::AppLock(configured, pending, error) => render_app_lock_settings(
                controls,
                configured,
                pending,
                error,
                Rc::new(move |dialog, window, cx| {
                    window_action(SettingsPageWindowAction::AppLock(dialog), window, cx)
                }),
                cx,
            ),
            SettingsPageData::KnownHosts => controls.settings_known_hosts_section(
                state,
                move |path, cx| action(SettingsPageAction::OpenKnownHostsFile(path), cx),
                cx,
            ),
            SettingsPageData::Proxy => ProxyPage {
                controls,
                state,
                dispatch: Rc::new(move |request, cx| {
                    action(SettingsPageAction::Proxy(request), cx)
                }),
            }
            .section(section, cx),
            SettingsPageData::Mcp(frame, target_label) => {
                let page = McpSettingsPage {
                    controls,
                    state,
                    frame,
                    dispatch: Rc::new(move |request, cx| {
                        action(SettingsPageAction::Mcp(request), cx)
                    }),
                    target_label: &*target_label,
                    scroll: &scroll("public-mcp-client-editor-scroll"),
                };
                if matches!(dialog, Some(SettingsDialog::McpClient)) {
                    return page.render_public_mcp_client_editor_modal(cx);
                }
                page.settings_public_mcp_section(cx)
            }
            SettingsPageData::General {
                data_dir,
                history_failed,
                command_specs,
            } => {
                use SettingsSectionId::*;
                let focus_handoff = (section == TerminalCommandBarFocusHandoff).then(|| {
                    let input = crate::SettingsInput::TerminalCommandBarFocusHandoff;
                    let value = oxideterm_settings_model::persisted_settings_input_value(
                        controls.settings,
                        input,
                    )
                    .unwrap_or_default();
                    let frame = controls.ime.frame(input, &value, cx);
                    let handle = scroll(&format!("multiline-input-{}", frame.anchor_id.0));
                    let font = crate::components::settings_mono_font_family(controls.settings);
                    let field = crate::components::settings_multiline_field(
                        controls.tokens,
                        frame,
                        font.clone(),
                        &handle,
                        controls.dispatch.clone(),
                    );
                    render_focus_handoff_commands(
                        &controls.components,
                        font,
                        field,
                        Rc::new(move |cmd, cx| {
                            action(SettingsPageAction::ToggleFocusHandoff(cmd), cx)
                        }),
                    )
                });
                let (command_specs_path, command_specs_summary) = command_specs;
                let page = GeneralPage {
                    controls,
                    state,
                    data_dir,
                    history_failed,
                    focus_handoff,
                    command_specs_path,
                    command_specs_summary,
                    dispatch: Rc::new(move |request, window, cx| {
                        window_action(SettingsPageWindowAction::General(request), window, cx)
                    }),
                };
                if matches!(
                    section,
                    TerminalCommandBarControls
                        | TerminalCommandBarFocusHandoff
                        | TerminalCommandBarQuickCommands
                        | TerminalAwarenessControls
                ) {
                    page.terminal_awareness_section(section, cx)
                } else {
                    page.settings_general_section(section, cx)
                }
            }
            SettingsPageData::Appearance(data, prepared, is_built_in) => {
                let appearance = data.page(
                    controls,
                    Rc::new({
                        let action = action.clone();
                        move |request, cx| action(SettingsPageAction::Appearance(request), cx)
                    }),
                );
                match prepared {
                    Some(PreparedSettingsPage::Background {
                        settings,
                        preview,
                        animated,
                    }) => BackgroundPage {
                        appearance,
                        state,
                        preview,
                        animated_media: animated,
                        is_built_in: &*is_built_in,
                        dispatch: Rc::new(move |request, window, cx| {
                            window_action(SettingsPageWindowAction::Background(request), window, cx)
                        }),
                        persist: Rc::new(move |edit, cx| {
                            action(SettingsPageAction::EditBackground(edit), cx)
                        }),
                    }
                    .render(&settings, cx),
                    Some(PreparedSettingsPage::ThemePreview(preview)) => {
                        appearance.section(section, appearance.settings, Some(preview), cx)
                    }
                    _ => appearance.section(section, appearance.settings, None, cx),
                }
            }
            SettingsPageData::LocalTerminal {
                shells,
                focused_input,
                credential_count,
            } => {
                let page = LocalTerminalPage {
                    controls,
                    shells,
                    focused_input,
                    privilege_credential_count: credential_count,
                    dispatch: Rc::new(move |request, window, cx| {
                        window_action(SettingsPageWindowAction::LocalTerminal(request), window, cx)
                    }),
                };
                if section == SettingsSectionId::ConnectionsReconnect {
                    page.settings_reconnect_section(0, cx)
                } else {
                    page.settings_local_section(section, cx)
                }
            }
            SettingsPageData::Help(year) => HelpPage {
                controls,
                state,
                year: &year,
                dispatch: Rc::new(move |request, cx| action(SettingsPageAction::Help(request), cx)),
            }
            .render(section, cx),
            SettingsPageData::Portable(path) => {
                let current_data_dir = path.parent().unwrap_or(path).display().to_string();
                PortableSettingsPage {
                    components: controls.components,
                    state: state.read(cx),
                    current_data_dir: &current_data_dir,
                    text: &|value, color| text(KeybindingTextRole::NonSelectable, 0, value, color),
                    dispatch: Rc::new(move |request, cx| {
                        action(SettingsPageAction::Portable(request), cx)
                    }),
                }
                .render_section(section)
            }
            SettingsPageData::RemoteShell(title, snapshot) => RemoteShellPage {
                components: controls.components,
                dispatch: Rc::new(move |request, cx| {
                    action(SettingsPageAction::RemoteShell(request), cx)
                }),
            }
            .card(title, snapshot, cx),
            SettingsPageData::Triggers(runtime) => {
                let trigger_state = &state.read(cx).terminal_triggers;
                let page = TerminalTriggerPage {
                    controls,
                    terminal_triggers: trigger_state,
                    connection_store: runtime.store,
                    terminal_trigger_shell_confirmation_pending: trigger_state
                        .shell_confirmation_pending,
                    modal_scroll: trigger_state.modal_scroll.clone(),
                    pane_exists: &*runtime.pane_exists,
                    applies_to_pane: &*runtime.applies,
                    enabled_for_pane: &*runtime.enabled,
                    quick_command_label: &*runtime.command_label,
                    dispatch_trigger: Rc::new(move |request, window, cx| {
                        window_action(SettingsPageWindowAction::Trigger(request), window, cx)
                    }),
                };
                if matches!(dialog, Some(SettingsDialog::Trigger)) {
                    return Some(page.render_terminal_trigger_editor_modal(cx));
                }
                match trigger_state.pane {
                    Some(pane) => page.terminal_trigger_session_card(pane, cx),
                    None => page.terminal_trigger_global_card(cx),
                }
            }
            SettingsPageData::Highlight => {
                let page = HighlightPage {
                    controls,
                    state: &state.read(cx).highlight,
                    dispatch: Rc::new(move |request, window, cx| {
                        window_action(SettingsPageWindowAction::Highlight(request), window, cx)
                    }),
                };
                if matches!(dialog, Some(SettingsDialog::Highlight)) {
                    return Some(
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .size_full()
                            .children(page.render_semantic_scheme_editor_modal(cx))
                            .children(page.render_highlight_rule_editor_modal(cx))
                            .into_any_element(),
                    );
                }
                page.section(section, cx)
            }
            SettingsPageData::Keybindings(catalog) => {
                let search = |cx: &App| {
                    let state = state.read(cx);
                    let input = crate::SettingsInput::KeybindingSearch;
                    controls.components.keybinding_search_field(
                        controls
                            .ime
                            .frame(input, state.keybinding_search_query(), cx),
                        controls.dispatch.clone(),
                    )
                };
                KeybindingPage {
                    components: controls.components.clone(),
                    state,
                    catalog: &catalog,
                    search: &search,
                    label: &*text,
                    dispatch: Rc::new(move |request, cx| {
                        action(SettingsPageAction::Keybindings(request), cx)
                    }),
                }
                .settings_keybindings_section(section, cx)
            }
            SettingsPageData::Knowledge(content, data) => {
                let model = oxideterm_settings_model::persisted_settings_input_value(
                    controls.settings,
                    crate::SettingsInput::AiEmbeddingModel,
                )
                .unwrap_or_default();
                content.render(
                    &KnowledgeSettingsPage {
                        controls,
                        frame: KnowledgeSettingsFrame {
                            embedding_model: &model,
                            ..data.frame
                        },
                        dispatch: Rc::new(move |request, pos, window, cx| {
                            window_action(
                                SettingsPageWindowAction::Knowledge(request, pos),
                                window,
                                cx,
                            )
                        }),
                        provider_has_key: &*data.provider_has_key,
                        document_selected: &*data.selected,
                        document_cut: &*data.cut,
                        chevron: &|expanded| (data.chevron)(expanded, cx),
                    },
                    cx,
                )
            }
            SettingsPageData::Sftp => controls.settings_sftp_section(section, cx),
            SettingsPageData::Ide => {
                controls.settings_ide_section(section, state.read(cx).ide_cjk_font_custom, cx)
            }
            SettingsPageData::Terminal(terminal) => {
                controls.terminal_settings_section(terminal, cx)
            }
        })
    }
}
