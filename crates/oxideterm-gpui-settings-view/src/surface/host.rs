use std::{path::Path, rc::Rc};

use gpui::{AnyElement, App, Context, Entity, Rgba, ScrollHandle, WeakEntity, Window};
use oxideterm_connections::ConnectionStore;
use oxideterm_gpui_ui::{ConfirmDialogAction, typography::tauri_ui_font_family};
use oxideterm_settings_model::{
    SettingsDynamicSectionCounts, SettingsNavigationLayout, SettingsSectionId,
};

use super::{
    KnowledgePageData, PreparedSettingsPage, SettingsPageAction, SettingsPageData,
    SettingsPageFrame, SettingsPageWindowAction, SettingsSurfaceBindings, SettingsSurfaceFrame,
    SettingsSurfaceRequest, TriggerPageRuntime,
};
use crate::{
    SettingsInput, SettingsTab, appearance_page::AppearancePageData, components::SettingsControls,
    entity::SettingsWorkspaceEntity, keybinding_page::KeybindingTextRole,
    knowledge_page::KnowledgeSection, mcp_page::McpSettingsFrame,
    remote_shell::RemoteShellIntegrationCardSnapshot, terminal_page::TerminalSettingsSection,
};

pub fn settings_controls_source<H: SettingsSurfaceHost>(
    owner: WeakEntity<H>,
) -> crate::components::SettingsControlsSource {
    Rc::new(move |cx| {
        let owner = owner.upgrade()?;
        Some(owner.read(cx).controls(owner, cx))
    })
}

pub type SettingsDocumentText = Rc<dyn Fn(KeybindingTextRole, u64, String, u32) -> AnyElement>;
pub type McpTargetLabel = Rc<dyn Fn(&oxideterm_public_mcp::ClientRef, &str, &App) -> String>;

/// Supplies live application data and accepts operations that belong to the window or runtime.
pub trait SettingsSurfaceHost: Sized + 'static {
    fn ai_providers(&self) -> Entity<crate::ai::providers::AiProviderSettingsView>;
    fn ai_mcp(&self) -> Entity<crate::ai::mcp::AiMcpSettingsView>;
    fn ai_skills(&self) -> crate::ai::AiSkillData;
    fn settings_state(&self) -> &Entity<SettingsWorkspaceEntity>;
    fn controls(&self, owner: Entity<Self>, cx: &App) -> SettingsControls<'_>;
    fn document_text(&self, owner: Entity<Self>, cx: &App) -> SettingsDocumentText;
    fn scroll_handle(&self, key: &str) -> ScrollHandle;
    fn layout(&self, cx: &App) -> (SettingsDynamicSectionCounts, Vec<u64>, Rgba);
    fn footer_focus(&self) -> Option<ConfirmDialogAction>;
    fn connections(&self) -> &ConnectionStore;
    fn settings_path(&self) -> &Path;
    fn data_directory(&self) -> oxideterm_settings::DataDirectoryInfo;
    fn history_failed(&self) -> bool;
    fn command_specs(&self) -> (String, String);
    fn local_shells(&self) -> Vec<oxideterm_terminal::ShellInfo>;
    fn focused_input(&self) -> Option<(SettingsInput, &str)>;
    fn keybindings(&self, cx: &App) -> Vec<crate::keybindings::ActionDefinition>;
    fn app_lock(&self) -> (bool, bool, Option<String>);
    fn appearance(&self, cx: &App) -> AppearancePageData;
    fn background_is_bundled(&self) -> Rc<dyn Fn(&Path) -> bool>;
    fn remote_shell(&self, cx: &App) -> (Option<String>, RemoteShellIntegrationCardSnapshot);
    fn triggers<'a>(&'a self, owner: WeakEntity<Self>) -> TriggerPageRuntime<'a>;
    fn public_mcp(&self, owner: WeakEntity<Self>) -> (McpSettingsFrame<'_>, McpTargetLabel);
    fn knowledge<'a>(&'a self, owner: WeakEntity<Self>, cx: &'a App) -> KnowledgePageData<'a>;
    fn knowledge_available(&self, cx: &App) -> bool;
    fn terminal_font(&self, cx: &App) -> crate::components::TerminalPreviewFont;

    fn prepare_portable(&mut self, cx: &mut Context<Self>);
    fn prepare_knowledge(
        &mut self,
        section: SettingsSectionId,
        cx: &mut Context<Self>,
    ) -> KnowledgeSection;
    fn active_theme_preview(&self, cx: &App) -> Option<String>;
    fn keybinding_note(&mut self, note: String, color: u32, cx: &mut Context<Self>) -> AnyElement;
    fn on_scroll(&mut self, cx: &mut Context<Self>);
    fn surface_request(
        &mut self,
        request: SettingsSurfaceRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    );
    fn page_action(&mut self, action: SettingsPageAction<'_>, cx: &mut Context<Self>);
    fn window_action(
        &mut self,
        action: SettingsPageWindowAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    );
}

impl SettingsSurfaceBindings {
    pub fn for_host<H: SettingsSurfaceHost>(weak: WeakEntity<H>) -> Self {
        macro_rules! bind {
            (|$owner:ident, $($arg:ident),+| $body:expr) => {{
                let $owner = weak.clone();
                Rc::new(move |$($arg),+| $body)
            }};
        }
        Self {
            frame: bind!(|weak, cx| {
                let entity = weak.upgrade()?;
                let host = entity.read(cx);
                let controls = host.controls(entity, cx);
                let (counts, signatures, divider) = host.layout(cx);
                Some(SettingsSurfaceFrame {
                    tokens: *controls.tokens,
                    i18n: controls.i18n.clone(),
                    has_background: controls.has_background,
                    ui_font: tauri_ui_font_family(&controls.settings.appearance.ui_font_family),
                    groups: SettingsNavigationLayout::from_persisted_groups(
                        &controls.settings.settings_navigation.groups,
                    )
                    .groups()
                    .to_vec(),
                    counts,
                    signatures,
                    divider,
                })
            }),
            page: bind!(|weak, section, prepared, cx| {
                SettingsPageFrame::from_host(weak.upgrade()?, section, prepared, cx)
            }),
            prepare_page: bind!(|weak, section, cx| {
                use SettingsSectionId::*;
                match section {
                    PortableRuntime => {
                        let _ = weak.update(cx, |host, cx| host.prepare_portable(cx));
                        None
                    }
                    KnowledgeError
                    | KnowledgeCollections
                    | KnowledgeDocuments
                    | KnowledgeEmbeddingConfig => weak
                        .update(cx, |host, cx| host.prepare_knowledge(section, cx))
                        .ok()
                        .map(PreparedSettingsPage::Knowledge),
                    AppearanceTheme | AppearanceBackground => {
                        prepare_appearance(weak.upgrade()?, section, cx)
                    }
                    _ => None,
                }
            }),
            scrolled: bind!(|weak, cx| {
                let _ = weak.update(cx, |host, cx| host.on_scroll(cx));
            }),
            keybinding_note: bind!(|weak, note, color, cx| weak
                .update(cx, |host, cx| host.keybinding_note(note, color, cx))
                .ok()),
            dispatch: bind!(|weak, request, window, cx| {
                let _ = weak.update(cx, |host, cx| host.surface_request(request, window, cx));
            }),
            search_field: bind!(|weak, value, focused, cx| {
                let owner = weak.upgrade()?;
                let controls = owner.read(cx).controls(owner, cx);
                let mut ime = controls.ime.frame(SettingsInput::SettingsSearch, value, cx);
                ime.focused = focused;
                Some(controls.navigation_search_field(ime, controls.dispatch.clone()))
            }),
            knowledge_available: bind!(|weak, cx| weak
                .upgrade()
                .is_some_and(|host| host.read(cx).knowledge_available(cx))),
        }
    }
}

fn prepare_appearance<H: SettingsSurfaceHost>(
    owner: Entity<H>,
    section: SettingsSectionId,
    cx: &mut App,
) -> Option<PreparedSettingsPage> {
    use crate::appearance_page::AppearancePreview;
    let host = owner.read(cx);
    let state = host.settings_state().clone();
    let hovered = host.active_theme_preview(cx);
    let controls = host.controls(owner, cx);
    let (tokens, i18n, settings, open_select) = (
        *controls.tokens,
        controls.i18n.clone(),
        controls.settings.clone(),
        controls.open_select,
    );
    let preview = AppearancePreview {
        tokens,
        i18n: &i18n,
        state: &state,
        open_settings_select: open_select,
        hovered_theme: hovered.as_deref(),
    };
    if section == SettingsSectionId::AppearanceTheme {
        let player = state.read(cx).theme_background_preview.clone();
        return Some(PreparedSettingsPage::ThemePreview(
            preview.render(&settings, player, true, cx),
        ));
    }
    let resolved = state.read(cx).background_settings_for_controls(&settings);
    let animated = state.update(cx, |state, cx| {
        state.background_media_is_animated(resolved.terminal.background_image.as_deref(), cx)
    });
    let visual = state.read(cx).theme_editor().is_none().then(|| {
        let player = state.read(cx).background_preview.clone();
        preview.render(&settings, player, true, cx)
    });
    Some(PreparedSettingsPage::Background {
        settings: Box::new(resolved),
        preview: visual,
        animated,
    })
}

impl<'a> SettingsPageFrame<'a> {
    pub fn from_host<H: SettingsSurfaceHost>(
        owner: Entity<H>,
        section: SettingsSectionId,
        prepared: Option<PreparedSettingsPage>,
        cx: &'a App,
    ) -> Option<Self> {
        use SettingsSectionId::*;
        let weak = owner.downgrade();
        let host = owner.read(cx);
        let state = host.settings_state().read(cx);
        let data = match section {
            AiGeneralSettings | AiGeneralPrivacy | AiToolsWebSearch => SettingsPageData::AiBasic,
            AiContextControls | AiContextSystemPrompt | AiContextMemory | AiToolsToolUse => {
                SettingsPageData::AiContext
            }
            AiContextModelWindows => {
                SettingsPageData::AiContextWindows(settings_controls_source(weak.clone()))
            }
            AiProvidersSettings => SettingsPageData::AiProviders(host.ai_providers()),
            AiToolsMcpServers => SettingsPageData::AiMcp(host.ai_mcp()),
            AiToolsAgents => SettingsPageData::AiSubagents,
            AiToolsSkills => SettingsPageData::AiSkills(host.ai_skills()),
            GeneralAppLock => {
                let (configured, pending, error) = host.app_lock();
                SettingsPageData::AppLock(configured, pending, error)
            }
            GeneralStartup
            | GeneralWindowBehavior
            | GeneralLanguage
            | GeneralExternalConnectionUris
            | GeneralDataDirectory
            | GeneralCliCompanion
            | TerminalCommandBarControls
            | TerminalCommandBarFocusHandoff
            | TerminalCommandBarQuickCommands
            | TerminalAwarenessControls => SettingsPageData::General {
                data_dir: (section == GeneralDataDirectory).then(|| host.data_directory()),
                history_failed: host.history_failed(),
                command_specs: if section == TerminalCommandBarQuickCommands {
                    host.command_specs()
                } else {
                    Default::default()
                },
            },
            TerminalLocalShell
            | TerminalLocalShellProfile
            | TerminalLocalPrivilegeCredentials
            | ConnectionsReconnect => SettingsPageData::LocalTerminal {
                shells: host.local_shells(),
                focused_input: host.focused_input(),
                credential_count: host
                    .connections()
                    .list_privilege_credentials(
                        oxideterm_connections::LOCAL_SHELL_PRIVILEGE_CONNECTION_ID,
                    )
                    .map(|items| items.len())
                    .unwrap_or_default(),
            },
            PortableRuntime => SettingsPageData::Portable(host.settings_path()),
            KeybindingsToolbar | KeybindingsScope(_) => {
                SettingsPageData::Keybindings(host.keybindings(cx))
            }
            KnowledgeError
            | KnowledgeCollections
            | KnowledgeDocuments
            | KnowledgeEmbeddingConfig => {
                let Some(PreparedSettingsPage::Knowledge(content)) = prepared else {
                    return None;
                };
                SettingsPageData::Knowledge(content, host.knowledge(weak.clone(), cx))
            }
            AppearanceTheme | AppearanceBackground | AppearanceLayout | AppearanceEffects
            | AppearanceAppIcon => SettingsPageData::Appearance(
                host.appearance(cx),
                prepared,
                host.background_is_bundled(),
            ),
            TerminalAwarenessShellIntegration => {
                let (title, snapshot) = host.remote_shell(cx);
                SettingsPageData::RemoteShell(title, snapshot)
            }
            TerminalAwarenessTriggers => SettingsPageData::Triggers(host.triggers(weak.clone())),
            NetworkPublicMcp => {
                let (frame, label) = host.public_mcp(weak.clone());
                SettingsPageData::Mcp(frame, label)
            }
            ConnectionsDefaults
            | ConnectionsIdleTimeout
            | ConnectionsRetention
            | ConnectionsSshKeys
            | ConnectionsSshConfigImport
            | ConnectionsImporters => {
                SettingsPageData::Connections(host.connections(), host.footer_focus())
            }
            ConnectionsKnownHosts => SettingsPageData::KnownHosts,
            PrivilegeCredentials | PrivilegeTotp => {
                SettingsPageData::Credentials(host.connections())
            }
            NetworkSharedProxy | NetworkRouting => SettingsPageData::Proxy,
            HelpVersion | HelpDiagnostics | HelpTechStack | HelpResources | HelpSafety
            | HelpLegal => SettingsPageData::Help(chrono::Local::now().format("%Y").to_string()),
            TerminalHighlightSelection | TerminalHighlightSemantic | TerminalHighlightRules => {
                SettingsPageData::Highlight
            }
            _ => match state.route_snapshot().active_tab {
                SettingsTab::Sftp => SettingsPageData::Sftp,
                SettingsTab::Ide => SettingsPageData::Ide,
                SettingsTab::Terminal => SettingsPageData::Terminal(match section {
                    TerminalDisplayFont => TerminalSettingsSection::Font {
                        custom_cjk: state.terminal_cjk_font_custom,
                        preview: host.terminal_font(cx),
                    },
                    TerminalDisplayBehavior => TerminalSettingsSection::DisplayBehavior,
                    TerminalDisplayCursor => TerminalSettingsSection::Cursor,
                    TerminalDisplayCommandMarks => TerminalSettingsSection::CommandMarks,
                    TerminalDisplayBuffer => TerminalSettingsSection::Buffer,
                    TerminalInputClipboard => TerminalSettingsSection::Clipboard,
                    TerminalInputMouseLinks => TerminalSettingsSection::MouseLinks,
                    TerminalInputKeyboard => TerminalSettingsSection::Keyboard,
                    TerminalTransferInBand => TerminalSettingsSection::InBandTransfer,
                    TerminalTransferXmodem => TerminalSettingsSection::Xmodem,
                    TerminalLoggingSession => TerminalSettingsSection::SessionLogging {
                        default_directory:
                            crate::terminal_page::terminal_session_log_root_directory(
                                host.settings_path(),
                                None,
                            )
                            .to_string_lossy()
                            .into_owned(),
                    },
                    _ => return None,
                }),
                _ => return None,
            },
        };

        Some(Self {
            text: host.document_text(owner.clone(), cx),
            controls: host.controls(owner, cx),
            data,
            scroll: Rc::new({
                let weak = weak.clone();
                move |key, cx| {
                    weak.upgrade()
                        .map(|host| host.read(cx).scroll_handle(key))
                        .unwrap_or_default()
                }
            }),
            action: Rc::new({
                let weak = weak.clone();
                move |action, cx| {
                    let _ = weak.update(cx, |host, cx| host.page_action(action, cx));
                }
            }),
            window_action: Rc::new(move |action, window, cx| {
                let _ = weak.update(cx, |host, cx| host.window_action(action, window, cx));
            }),
        })
    }
}
