//! Mount with `cx.new(|cx| PortableSettingsSurface::new(settings, actions, cx))`.
//! The host supplies its actual action catalog. Subscribe to `PortableSettingsChanged`
//! and read `surface.settings(cx)` to apply or persist accepted edits.
use std::{
    cell::Cell,
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    rc::Rc,
};

use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, FocusHandle, KeyDownEvent, MouseButton, Render,
    ScrollHandle, Subscription, Window, WindowAppearance, div, prelude::*, px, rgb,
};
use oxideterm_gpui_ui::select::{OverlayAnchor, SelectAnchorId};
use oxideterm_i18n::{I18n, Locale};
use oxideterm_settings::{Language, PersistedSettings};
use oxideterm_settings_model::{
    AiSettingsPage, SETTINGS_SECTION_HEADER_ITEM_COUNT, SettingsDynamicSectionCounts,
    SettingsNavigationLayout, SettingsSectionId, ThemeEditorState, settings_tab_sections,
};
use oxideterm_theme::ThemeTokens;

use super::{
    NavigationState, SettingsRouteOwner, SettingsSectionList, SettingsSurfaceFrame,
    SettingsSurfaceRequest, TERMINAL_PAGE_SWITCHER, nav, page_switcher,
    search_filter::SettingsSearchFilter, settings_content, settings_layout,
};
use crate::{
    SettingsInput, SettingsRouteState, SettingsSearchResult, SettingsSelect,
    SettingsSelectAnchorExt, SettingsSlider, SettingsTab, TerminalSettingsPage,
    components::{SettingsComponents, SettingsControls, SettingsImeSource},
    keybinding_page::KeybindingState,
    keybindings::ActionDefinition,
    theme_picker::SettingsThemePicker,
};

mod actions;
mod fields;
mod wallpaper;
use oxideterm_settings_model::browser_preferences::{BundledWallpaper,WallpaperId};
mod host;
pub use host::{PortableKnownHostsAction, PortableProxyProbe};
mod pages;
use fields::Fields;

/// A persisted preference changed; navigation and draft-only edits do not emit this event.
pub struct PortableSettingsChanged;

/// Mount with `cx.new(|cx| PortableSettingsSurface::new(settings, action_catalog, cx))`.
/// Subscribe to [`PortableSettingsChanged`] and read [`Self::settings`] to persist edits.
/// The catalog defines the shortcuts the host actually implements.
pub struct PortableSettingsSurface {
    state: Entity<PageState>,
    picker: Entity<SettingsThemePicker>,
    ai_providers: Entity<crate::ai::providers::AiProviderSettingsView>,
    _observe: Subscription,
    _edits: Subscription,
}

impl EventEmitter<PortableSettingsChanged> for PortableSettingsSurface {}
impl EventEmitter<PortableSettingsChanged> for PageState {}

#[cfg(all(test, not(target_family = "wasm")))]
mod browser_tests {
    use super::*;

    #[gpui::test]
    fn browser_settings_open_search_hits_but_not_desktop_only_routes(
        cx: &mut gpui::TestAppContext,
    ) {
        let settings =
            cx.new(|cx| PortableSettingsSurface::browser_workspace(Default::default(), vec![], cx));
        let state = settings.read_with(cx, |s, _| s.state.clone());
        state.update(cx, |s, cx| {
            s.i18n.set_locale(Locale::En);
            s.search.apply("MCP Servers".into());
            assert!(
                !s.search
                    .results(&s.i18n)
                    .iter()
                    .any(|r| r.section == SettingsSectionId::AiToolsMcpServers)
            );
            s.search.apply("API Key".into());
            let results = s.search.results(&s.i18n);
            let hit = results
                .iter()
                .find(|r| r.section == SettingsSectionId::AiProvidersSettings)
                .unwrap();
            s.navigate(hit.tab, Some(hit), cx);
            s.navigate(SettingsTab::Portable, None, cx);
            let (frame, sections) = s.frame(true);
            assert_eq!(s.route.route_snapshot().active_tab, SettingsTab::Ai);
            assert_eq!(
                sections,
                [
                    SettingsSectionId::AiPageSwitcher,
                    SettingsSectionId::AiProvidersSettings
                ]
            );
            assert!(!frame.groups.iter().flatten().any(|tab| matches!(
                tab,
                SettingsTab::Portable | SettingsTab::Network | SettingsTab::Privilege
            )));
        });
    }
}

fn supported(tab: SettingsTab, section: SettingsSectionId) -> bool {
    use SettingsSectionId::*;
    match tab {
        SettingsTab::General => matches!(
            section,
            GeneralLanguage | GeneralExternalConnectionUris | GeneralWindowBehavior
        ),
        SettingsTab::Portable => section == PortableRuntime,
        SettingsTab::Connections => section == ConnectionsKnownHosts,
        SettingsTab::Discovery => false,
        SettingsTab::Privilege => section == PrivilegeCredentials,
        SettingsTab::Network => matches!(section, NetworkSharedProxy | NetworkRouting),
        SettingsTab::Help => matches!(
            section,
            HelpVersion | HelpDiagnostics | HelpTechStack | HelpResources | HelpSafety | HelpLegal
        ),
        SettingsTab::Appearance => matches!(
            section,
            AppearanceTheme | AppearanceLayout | AppearanceEffects | AppearanceAppIcon
        ),
        SettingsTab::Terminal => matches!(
            section,
            TerminalPageSwitcher
                | TerminalDisplayFont
                | TerminalDisplayBehavior
                | TerminalDisplayCursor
                | TerminalDisplayCommandMarks
                | TerminalDisplayBuffer
                | TerminalInputClipboard
                | TerminalInputMouseLinks
                | TerminalInputKeyboard
                | TerminalLocalShell
                | TerminalLocalShellProfile
                | TerminalLocalPrivilegeCredentials
                | TerminalTransferInBand
                | TerminalTransferXmodem
                | TerminalCommandBarControls
                | TerminalCommandBarFocusHandoff
                | TerminalCommandBarQuickCommands
                | TerminalAwarenessControls
                | TerminalAwarenessShellIntegration
                | TerminalAwarenessTriggers
                | TerminalLoggingSession
                | TerminalHighlightSelection
                | TerminalHighlightSemantic
                | TerminalHighlightRules
        ),
        SettingsTab::Sftp => matches!(section, SftpOpening | SftpPerformance | SftpConflict),
        SettingsTab::Ide => matches!(
            section,
            IdeEditing | IdeTypography | IdeAgent | IdeAgentPrivacy
        ),
        SettingsTab::Ai => matches!(
            section,
            AiPageSwitcher
                | AiGeneralSettings
                | AiGeneralPrivacy
                | AiProvidersSettings
                | AiContextControls
                | AiContextSystemPrompt
                | AiContextMemory
                | AiContextModelWindows
                | AiToolsToolUse
                | AiToolsWebSearch
                | AiToolsAgents
                | AiToolsSkills
                | AiToolsMcpServers
        ),
        SettingsTab::Knowledge => {
            matches!(section, KnowledgeCollections | KnowledgeEmbeddingConfig)
        }
        SettingsTab::Keybindings => matches!(section, KeybindingsToolbar | KeybindingsScope(_)),
    }
}

use crate::search::SettingsHost;
type SectionFilter = fn(SettingsTab, SettingsSectionId) -> bool;
type ReportDownload = Rc<dyn Fn(&mut Window, &mut App)>;

fn browser_section(tab: SettingsTab, section: SettingsSectionId) -> bool {
    use SettingsSectionId::*;
    supported(tab, section)
        && !matches!(
            section,
            PortableRuntime
                | PrivilegeCredentials
                | TerminalLocalShell
                | TerminalLocalShellProfile
                | TerminalLocalPrivilegeCredentials
                | TerminalLoggingSession
                | TerminalTransferInBand
                | TerminalTransferXmodem
                | GeneralWindowBehavior
                | GeneralExternalConnectionUris
                | ConnectionsKnownHosts
                | NetworkSharedProxy
                | NetworkRouting
                | IdeAgent
                | IdeAgentPrivacy
                | AiToolsAgents
                | AiToolsSkills
                | AiToolsMcpServers
                | AiToolsToolUse
                | AiToolsWebSearch
        )
}

#[derive(Default, PartialEq, Eq, Hash)]
pub enum PortableKnowledgeLibrary {
    #[default]
    Unavailable,
    Loading,
    Failed,
    Ready {
        collections: Vec<String>,
        keyword_ready: bool,
    },
}

pub enum WallpaperAction {
    Pick,
    Select(WallpaperId),
    Remove(WallpaperId),
    Clear,
    Reload,
}

#[derive(Clone)]
pub struct WallpaperTile {
    pub id: WallpaperId,
    pub name: String,
    pub format_key: &'static str,
    pub thumbnail: gpui::ImageSource,
}

#[derive(Clone)]
pub struct BrowserWallpaperGallery {
    pub request: Rc<dyn Fn(WallpaperAction, &mut App)>,
    pub source: fn(BundledWallpaper) -> gpui::ImageSource,
    pub supports_tab: fn(&str) -> bool,
}

struct PageState {
    host: SettingsHost,
    browser_report: Option<ReportDownload>,
    wallpaper_gallery: Option<BrowserWallpaperGallery>,
    wallpaper_tiles: Vec<WallpaperTile>,
    gallery_busy: bool,
    gallery_error: Option<&'static str>,
    loaded_wallpaper: Option<WallpaperId>,
    section_filter: SectionFilter,
    knowledge: PortableKnowledgeLibrary,
    settings: PersistedSettings,
    tokens_cache: ThemeTokens,
    catalog: Vec<ActionDefinition>,
    route: SettingsRouteState,
    navigation: NavigationState,
    list: SettingsSectionList,
    target: Cell<Option<SettingsSectionId>>,
    query: String,
    search: SettingsSearchFilter,
    search_open: bool,
    focus: FocusHandle,
    fields: Fields,
    select: Option<SettingsSelect>,
    anchors: HashMap<SelectAnchorId, OverlayAnchor>,
    slider: Option<SettingsSlider>,
    terminal_custom: bool,
    ide_custom: bool,
    scheme: Option<bool>,
    editor: Option<ThemeEditorState>,
    editor_scroll: ScrollHandle,
    known_hosts: host::KnownHosts,
    probe: host::ProxyProbe,
    ai_api_keys: HashMap<String, zeroize::Zeroizing<String>>,
    ai_api_key_drafts: HashMap<String, zeroize::Zeroizing<String>>,
    ai_new_model_drafts: HashMap<String, String>,
    ai_tool_policy_expanded: bool,
    ai_confirm: Entity<crate::ai::confirm::AiSettingsConfirm>,
    document: Option<(
        bool,
        oxideterm_gpui_markdown::MarkdownVirtualListScrollHandle,
    )>,
    shortcuts: KeybindingState,
    reset_open: bool,
    app_icon_picker: Option<(
        &'static [oxideterm_settings::AppIconVariant],
        f32,
        fn(oxideterm_settings::AppIconVariant) -> gpui::ImageSource,
    )>,
    i18n: I18n,
}

impl PortableSettingsSurface {
    pub fn new(
        settings: PersistedSettings,
        actions: Vec<ActionDefinition>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::with_sections(settings, actions, SettingsHost::Managed, None, cx)
    }

    /// Settings for the visiting browser, not for a separately managed desktop installation.
    pub fn browser_workspace(
        settings: PersistedSettings,
        actions: Vec<ActionDefinition>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::with_sections(settings, actions, SettingsHost::Browser, None, cx)
    }

    pub fn browser_workspace_with_wallpapers(
        settings: PersistedSettings,
        actions: Vec<ActionDefinition>,
        gallery: BrowserWallpaperGallery,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::with_sections(settings, actions, SettingsHost::Browser, Some(gallery), cx)
    }

    fn with_sections(
        settings: PersistedSettings,
        actions: Vec<ActionDefinition>,
        host: SettingsHost,
        wallpaper_gallery: Option<BrowserWallpaperGallery>,
        cx: &mut Context<Self>,
    ) -> Self {
        let section_filter: SectionFilter = match host {
            SettingsHost::Browser if wallpaper_gallery.is_some() => |tab, section| {
                browser_section(tab, section) || (tab == SettingsTab::Appearance && section == SettingsSectionId::AppearanceBackground)
            },
            SettingsHost::Browser => browser_section,
            _ => supported,
        };
        let initial_tokens = pages::tokens(&settings, true);
        let ai_confirm = cx.new(|_| {
            crate::ai::confirm::AiSettingsConfirm::new(
                initial_tokens,
                I18n::new(locale(settings.general.language)),
            )
        });
        let state = cx.new(|cx| PageState {
            host,
            wallpaper_gallery,
            wallpaper_tiles: Vec::new(),
            gallery_busy: false,
            gallery_error: None,
            loaded_wallpaper: None,
            browser_report: None,
            section_filter,
            i18n: I18n::new(locale(settings.general.language)),
            tokens_cache: initial_tokens,
            settings,
            catalog: actions,
            route: SettingsRouteState::default(),
            navigation: Default::default(),
            list: Default::default(),
            target: Cell::new(None),
            query: String::new(),
            search: SettingsSearchFilter::new(move |i18n, query| {
                crate::search::filtered_results(i18n, query, section_filter, host)
            }),
            search_open: false,
            focus: cx.focus_handle(),
            fields: Default::default(),
            select: None,
            anchors: Default::default(),
            slider: None,
            terminal_custom: false,
            ide_custom: false,
            scheme: None,
            editor: None,
            editor_scroll: ScrollHandle::new(),
            known_hosts: Default::default(),
            knowledge: Default::default(),
            probe: Default::default(),
            ai_api_keys: Default::default(),
            ai_api_key_drafts: Default::default(),
            ai_new_model_drafts: Default::default(),
            ai_tool_policy_expanded: false,
            ai_confirm: ai_confirm.clone(),
            document: None,
            shortcuts: Default::default(),
            reset_open: false,
            app_icon_picker: None,
        });
        cx.subscribe(&ai_confirm, {
            let state = state.clone();
            move |_, _, intent: &crate::ai::confirm::AiSettingsConfirmIntent, cx| {
                if *intent == crate::ai::confirm::AiSettingsConfirmIntent::Enable {
                    state.update(cx, |s, cx| {
                        crate::options::set_ai_enabled_confirmed(&mut s.settings, true);
                        crate::options::set_ai_enabled(&mut s.settings, true);
                        s.changed(cx);
                    });
                }
            }
        })
        .detach();
        let picker = actions::theme_picker(&state, cx);
        let ai_providers = actions::ai_providers_view(&state, &picker, cx);
        Self {
            _observe: cx.observe(&state, |_, _, cx| cx.notify()),
            _edits: cx.subscribe(&state, |_, _, _: &PortableSettingsChanged, cx| {
                cx.emit(PortableSettingsChanged)
            }),
            state,
            picker,
            ai_providers,
        }
    }

    pub fn captures_workspace_shortcuts(&self, cx: &App) -> bool {
        let state = self.state.read(cx);
        state.ai_confirm.read(cx).state().is_some() || state.reset_open
            || state.shortcuts.keybinding_recording_action_id().is_some()
    }

    pub fn settings<'a>(&self, cx: &'a App) -> &'a PersistedSettings {
        &self.state.read(cx).settings
    }

    pub fn has_provider_api_key(&self, id: &str, cx: &App) -> bool {
        self.state.read(cx).ai_api_keys.contains_key(id)
    }

    /// Returns the in-memory session API key configured for an AI provider, if any.
    pub fn provider_api_key(
        &self,
        provider_id: &str,
        cx: &App,
    ) -> Option<zeroize::Zeroizing<String>> {
        self.state.read(cx).ai_api_keys.get(provider_id).cloned()
    }

    /// Resolve the same palette and language for surfaces mounted outside settings.
    pub fn environment(&self, dark: bool, cx: &App) -> (ThemeTokens, I18n) {
        let state = self.state.read(cx);
        (pages::tokens(&state.settings, dark), state.i18n.clone())
    }

    pub fn set_loaded_wallpaper(&self, id: Option<WallpaperId>, cx: &mut App) {
        self.state.update(cx, |s, cx| {
            if s.loaded_wallpaper != id {
                s.loaded_wallpaper = id;
                cx.notify();
            }
        });
    }

    pub fn update_wallpaper_gallery(
        &self, tiles: Vec<WallpaperTile>, busy: bool, error: Option<&'static str>, cx: &mut App,
    ) {
        self.state.update(cx, |s, cx| {
            s.wallpaper_tiles = tiles;
            s.gallery_busy = busy;
            s.gallery_error = error;
            cx.notify();
        });
    }

    pub fn edit_preferences(&self, cx: &mut App, edit: impl FnOnce(&mut PersistedSettings)) {
        self.state.update(cx, |state, cx| {
            edit(&mut state.settings);
            state.changed(cx);
        });
    }

    pub fn open_ai_providers(&self, cx: &mut App) {
        self.state.update(cx, |s,cx| {
            s.route.set_ai_page(AiSettingsPage::Providers);
            s.navigate(SettingsTab::Ai,None,cx);
        });
    }

    pub fn open_section(&self, tab: SettingsTab, cx: &mut App) {
        self.state
            .update(cx, |state, cx| state.navigate(tab, None, cx));
    }

    pub fn knowledge_dialog(
        &self,
        tokens: &ThemeTokens,
        content: crate::knowledge_page::dialog::KnowledgeDialogContent<'_>,
        fields: Option<(AnyElement, AnyElement)>,
        focus: Option<oxideterm_gpui_ui::ConfirmDialogAction>,
        dispatch: Rc<dyn Fn(crate::knowledge_page::dialog::KnowledgeDialogAction, &mut App)>,
        cx: &App,
    ) -> AnyElement {
        let state = self.state.read(cx);
        crate::knowledge_page::dialog::KnowledgeDialog {
            controls: state.controls(
                tokens,
                actions::window_dispatch(self.state.downgrade(), self.picker.downgrade()),
            ),
            focus,
            dispatch,
        }
        .render_with_fields(content, fields, cx)
    }

    pub fn set_knowledge_library(&self, library: PortableKnowledgeLibrary, cx: &mut App) {
        self.state.update(cx, |s, cx| {
            if s.knowledge != library {
                s.knowledge = library;
                cx.notify();
            }
        });
    }

    pub fn on_browser_report(&self, download: impl Fn(&mut Window, &mut App) + 'static, cx: &mut App) {
        self.state.update(cx, |s, cx| {
            s.browser_report = Some(Rc::new(download));
            cx.notify();
        });
    }


    pub fn set_app_icon_picker(
        &self,
        variants: &'static [oxideterm_settings::AppIconVariant],
        size: f32,
        source: fn(oxideterm_settings::AppIconVariant) -> gpui::ImageSource,
        cx: &mut App,
    ) {
        self.state.update(cx, |state, cx| {
            state.app_icon_picker = Some((variants, size, source));
            cx.notify();
        });
    }
}

fn locale(language: Language) -> Locale {
    match language {
        Language::De => Locale::De,
        Language::En => Locale::En,
        Language::EsEs => Locale::EsEs,
        Language::FrFr => Locale::FrFr,
        Language::It => Locale::It,
        Language::Ja => Locale::Ja,
        Language::Ko => Locale::Ko,
        Language::PtBr => Locale::PtBr,
        Language::Vi => Locale::Vi,
        Language::ZhCn => Locale::ZhCn,
        Language::ZhTw => Locale::ZhTw,
    }
}

fn system_dark(window: &Window) -> bool {
    matches!(
        window.appearance(),
        WindowAppearance::Dark | WindowAppearance::VibrantDark
    )
}

impl PageState {
    fn edit_scheme(&self, dark: bool) -> Option<bool> {
        self.scheme.or_else(|| {
            self.settings
                .appearance
                .active_scheme(dark)
                .map(|s| s.is_dark())
        })
    }

    fn changed(&mut self, cx: &mut Context<Self>) {
        self.proxy_profile_changed();
        let language = locale(self.settings.general.language);
        if language != self.i18n.locale() {
            self.i18n.set_locale(language);
        }
        cx.emit(PortableSettingsChanged);
        cx.notify();
    }

    fn navigate(
        &mut self,
        tab: SettingsTab,
        result: Option<&SettingsSearchResult>,
        cx: &mut Context<Self>,
    ) {
        if let Some(result) = result {
            if !(self.section_filter)(tab, result.section) {
                return;
            }
        } else if self.sections_for(tab, self.section_counts()).is_empty() {
            return;
        }
        if tab != SettingsTab::Network {
            self.clear_proxy_password();
        }
        self.route.set_active_tab(tab);
        if let Some(page) = result.and_then(|r| r.terminal_page) {
            self.route.set_terminal_page(page);
        }
        if let Some(page) = result.and_then(|r| r.ai_page) {
            self.route.set_ai_page(page);
        }
        self.target.set(result.map(|r| r.section));
        self.search_open = false;
        self.query.clear();
        self.search.apply(String::new());
        self.fields.blur();
        self.select = None;
        self.slider = None;
        self.shortcuts.stop_keybinding_recording(cx);
        cx.notify();
    }

    fn has_wallpaper(&self) -> bool {
        let t = &self.settings.terminal;
        self.wallpaper_gallery.is_some()
            && t.background_enabled && WallpaperId::selected(t).is_some_and(|id|matches!(id,WallpaperId::Bundled(_))||self.loaded_wallpaper==Some(id))
            && (t.background_scope == oxideterm_settings::BackgroundScope::Window
                || t.background_scope
                    .includes_content(&t.background_enabled_tabs, "settings"))
    }

    fn controls<'a>(
        &'a self,
        tokens: &'a ThemeTokens,
        dispatch: crate::components::SettingsWindowDispatch,
    ) -> SettingsControls<'a> {
        SettingsControls {
            components: SettingsComponents {
                settings: &self.settings,
                tokens,
                i18n: &self.i18n,
                has_background: self.has_wallpaper(),
            },
            dispatch,
            ime: self,
            open_select: self.select,
            select_origin: Some(oxideterm_gpui_ui::select::BrowserFocusOrigin::Pointer),
            label: Rc::new(|_, _, value, color, _| {
                div().text_color(rgb(color)).child(value).into_any_element()
            }),
        }
    }

    fn section_counts(&self) -> SettingsDynamicSectionCounts {
        let route = self.route.route_snapshot();
        SettingsDynamicSectionCounts {
            terminal_page: route.terminal_page,
            ai_page: route.ai_page,
            visible_keybinding_scope_count: if route.active_tab == SettingsTab::Keybindings {
                crate::keybinding_page::scope_count(&self.catalog, &self.shortcuts, |a| {
                    a.label
                        .clone()
                        .unwrap_or_else(|| self.i18n.t(&a.label_key()))
                })
            } else {
                0
            },
            knowledge_has_error: false,
            knowledge_has_selected_collection: false,
        }
    }

    fn sections_for(
        &self,
        tab: SettingsTab,
        counts: SettingsDynamicSectionCounts,
    ) -> Vec<SettingsSectionId> {
        let mut sections = settings_tab_sections(tab, counts);
        // Managed desktop settings can include tray controls on a non-desktop client.
        if tab == SettingsTab::General
            && !sections.contains(&SettingsSectionId::GeneralWindowBehavior)
        {
            sections.push(SettingsSectionId::GeneralWindowBehavior);
        }

        sections.retain(|section| (self.section_filter)(tab, *section));
        sections
    }

    fn frame(&self, dark: bool) -> (SettingsSurfaceFrame, Vec<SettingsSectionId>) {
        let route = self.route.route_snapshot();
        let counts = self.section_counts();
        let layout = SettingsNavigationLayout::from_persisted_groups(
            &self.settings.settings_navigation.groups,
        );
        let groups = layout
            .groups()
            .iter()
            .filter_map(|group| {
                let tabs: Vec<_> = group
                    .iter()
                    .copied()
                    .filter(|&tab| !self.sections_for(tab, counts).is_empty())
                    .collect();
                (!tabs.is_empty()).then_some(tabs)
            })
            .collect();
        let sections = self.sections_for(route.active_tab, counts);
        let signatures = std::iter::once(None)
            .chain(sections.iter().copied().map(Some))
            .map(|section| {
                let mut h = DefaultHasher::new();
                section.hash(&mut h);
                self.i18n.locale().hash(&mut h);
                self.settings.appearance.ui_font_size.hash(&mut h);
                self.settings.appearance.ui_font_family.hash(&mut h);
                (self.settings.appearance.ui_density as u8).hash(&mut h);
                match section {
                    Some(
                        SettingsSectionId::KnowledgeCollections
                        | SettingsSectionId::KnowledgeEmbeddingConfig,
                    ) => self.knowledge.hash(&mut h),
                    Some(SettingsSectionId::ConnectionsKnownHosts) => {
                        self.known_hosts.revision.hash(&mut h);
                        self.known_hosts.query.hash(&mut h);
                        self.known_hosts.selected.hash(&mut h);
                        self.known_hosts.snapshot.loading.hash(&mut h);
                    }
                    Some(SettingsSectionId::NetworkSharedProxy) => {
                        self.settings
                            .network
                            .upstream_proxy
                            .as_ref()
                            .map(|p| {
                                matches!(
                                    p.auth,
                                    oxideterm_settings::SettingsUpstreamProxyAuth::Password { .. }
                                )
                            })
                            .hash(&mut h);
                        self.probe.handler.is_some().hash(&mut h);
                        self.probe.form.test_pending.hash(&mut h);
                        self.probe.form.test_result.hash(&mut h);
                    }
                    Some(SettingsSectionId::NetworkRouting) => {
                        self.settings.general.update_proxy.mode.hash(&mut h);
                    }
                    Some(SettingsSectionId::SftpPerformance) => {
                        self.settings.sftp.speed_limit_enabled.hash(&mut h)
                    }
                    Some(SettingsSectionId::TerminalDisplayFont) => {
                        self.terminal_custom.hash(&mut h);
                        (self.settings.terminal.font_family as u8).hash(&mut h);
                        self.settings.terminal.font_size.hash(&mut h);
                        self.settings.terminal.line_height.to_bits().hash(&mut h);
                        self.settings.terminal.cjk_font_family.hash(&mut h);
                    }
                    Some(SettingsSectionId::IdeTypography) => {
                        self.ide_custom.hash(&mut h);
                        self.settings.ide.font_family.map(|f| f as u8).hash(&mut h);
                        self.settings.ide.cjk_font_family.hash(&mut h);
                    }
                    Some(SettingsSectionId::KeybindingsScope(_)) => {
                        self.shortcuts.keybinding_search_query().hash(&mut h);
                        (self.shortcuts.keybinding_scope_filter() as u8).hash(&mut h);
                        self.shortcuts.keybinding_recording_action_id().hash(&mut h);
                        self.shortcuts.keybinding_conflicts().hash(&mut h);
                        self.shortcuts
                            .keybinding_recording_combo()
                            .is_some()
                            .hash(&mut h);
                    }
                    Some(SettingsSectionId::AppearanceBackground) => {
                        self.settings.terminal.background_image.hash(&mut h);
                        self.settings.terminal.background_enabled.hash(&mut h);
                        self.settings.terminal.background_opacity.to_bits().hash(&mut h);
                        (self.settings.terminal.background_scope as u8).hash(&mut h);
                        self.settings.terminal.background_enabled_tabs.hash(&mut h);
                        self.gallery_busy.hash(&mut h);
                        self.gallery_error.hash(&mut h);
                        for tile in &self.wallpaper_tiles { tile.id.hash(&mut h); }
                    }
                    Some(SettingsSectionId::AppearanceTheme) => {
                        self.settings
                            .appearance
                            .follow_system_appearance
                            .hash(&mut h);
                        self.settings.appearance.manual_scheme.hash(&mut h);
                        for target in [
                            oxideterm_settings_model::ThemeTarget::Application,
                            oxideterm_settings_model::ThemeTarget::Terminal,
                        ] {
                            target
                                .scheme_id(&self.settings, self.scheme.unwrap_or(dark))
                                .hash(&mut h);
                        }
                    }
                    Some(SettingsSectionId::AiProvidersSettings) => {
                        self.settings.ai.enabled.hash(&mut h);
                        self.settings.ai.providers.len().hash(&mut h);
                        self.settings.ai.active_provider_id.hash(&mut h);
                        self.ai_api_keys.len().hash(&mut h);
                    }
                    Some(SettingsSectionId::AiGeneralSettings) => {
                        self.settings.ai.enabled.hash(&mut h);
                    }
                    Some(SettingsSectionId::AiToolsToolUse) => {
                        self.settings.ai.enabled.hash(&mut h);
                        self.settings.ai.tool_use.enabled.hash(&mut h);
                        self.ai_tool_policy_expanded.hash(&mut h);
                    }
                    Some(
                        SettingsSectionId::AiContextControls
                        | SettingsSectionId::AiContextSystemPrompt
                        | SettingsSectionId::AiContextMemory
                        | SettingsSectionId::AiContextModelWindows
                        | SettingsSectionId::AiToolsWebSearch
                        | SettingsSectionId::AiToolsAgents
                        | SettingsSectionId::AiToolsSkills
                        | SettingsSectionId::AiToolsMcpServers,
                    ) => {
                        self.settings.ai.enabled.hash(&mut h);
                    }
                    _ => {}
                }
                h.finish()
            })
            .collect();
        let tokens = pages::tokens(&self.settings, dark);
        (
            SettingsSurfaceFrame {
                counts,
                signatures,
                tokens,
                i18n: self.i18n.clone(),
                has_background: self.has_wallpaper(),
                ui_font: oxideterm_gpui_ui::typography::tauri_ui_font_family(
                    &self.settings.appearance.ui_font_family,
                ),
                divider: gpui::rgba((tokens.ui.border << 8) | if self.has_wallpaper() { 0x26 } else { 0x66 }),
                groups,
            },
            sections,
        )
    }
}

impl SettingsRouteOwner for PageState {
    fn navigation_mut(&mut self) -> &mut NavigationState {
        &mut self.navigation
    }

    fn active_tab(&self, _: &App) -> SettingsTab {
        self.route.route_snapshot().active_tab
    }

    fn select_tab(&mut self, tab: SettingsTab, _: &mut Window, cx: &mut Context<Self>) {
        self.navigate(tab, None, cx);
    }
}

impl PageState {
    fn navigation_request(&mut self, request: SettingsSurfaceRequest, cx: &mut Context<Self>) {
        match request {
            SettingsSurfaceRequest::ToggleSearch | SettingsSurfaceRequest::ClearSearch => {
                let open =
                    !self.search_open || matches!(request, SettingsSurfaceRequest::ClearSearch);
                self.fields.blur();
                self.query.clear();
                self.search.apply(String::new());
                self.search_open = open;
                if open {
                    self.focus_input(SettingsInput::SettingsSearch);
                }
            }
            SettingsSurfaceRequest::SearchResult(result) => {
                return self.navigate(result.tab, Some(&result), cx);
            }
            SettingsSurfaceRequest::SelectedTab(tab) => return self.navigate(tab, None, cx),
            SettingsSurfaceRequest::OpenNavigationEditor | SettingsSurfaceRequest::Tooltip(..) => {
                unreachable!("desktop toolbar actions use host bindings")
            }
        }
        cx.notify();
    }
}

impl Render for PortableSettingsSurface {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dark = system_dark(window);
        let refreshed_tokens = pages::tokens(&self.state.read(cx).settings, dark);
        self.state.update(cx, |s, _| {
            s.tokens_cache = refreshed_tokens;
        });
        let state = self.state.read(cx);
        let (frame, sections) = state.frame(dark);
        let tab = state.route.route_snapshot().active_tab;
        state.list.sync(tab, frame.counts, &frame.signatures);
        if let Some(target) = state.target.take()
            && let Some(index) = sections.iter().position(|s| *s == target)
        {
            state.list.state().scroll_to(gpui::ListOffset {
                item_ix: index + SETTINGS_SECTION_HEADER_ITEM_COUNT,
                offset_in_item: px(0.0),
            });
        }
        let tokens = frame.tokens;
        let dispatch = actions::window_dispatch(self.state.downgrade(), self.picker.downgrade());
        let cached;
        let search = if state.search_open {
            let controls = state.controls(&tokens, dispatch.clone());
            let input = controls.navigation_search_field(
                SettingsImeSource::frame(state, SettingsInput::SettingsSearch, &state.query, cx),
                dispatch.clone(),
            );
            cached = state.search.results(&frame.i18n);
            Some(nav::SearchPanel {
                query: &state.query,
                input: Some(input),
                results: cached.iter().collect(),
            })
        } else {
            None
        };
        let owner = self.state.downgrade();
        let nav = nav::Sidebar {
            state: &state.navigation,
            owner: owner.clone(),
            active_tab: tab,
            desktop_toolbar: false,
            dispatch: Rc::new(move |request, _, cx| {
                let _ = owner.update(cx, |s, cx| s.navigation_request(request, cx));
            }),
        }
        .render(&frame, search);
        let list = state.list.state().clone();
        let owner = self.state.clone();
        let focus = state.focus.clone();
        let ime = state.fields.active.is_some();
        let ui_font = frame.ui_font.clone();
        let has_background = frame.has_background;
        let scroll_owner = self.state.downgrade();
        let section_dispatch = dispatch.clone();
        let ai_providers = self.ai_providers.clone();
        let content = settings_content(
            frame,
            tab,
            list,
            move |index| sections.get(index).copied(),
            |s, _, _| Some(div().child(s).into_any_element()),
            move |cx| {
                let _ = scroll_owner.update(cx, |s, cx| {
                    s.anchors.clear();
                    if s.select.take().is_some() {
                        cx.notify();
                    }
                });
            },
            move |frame, section, cx| {
                if section == SettingsSectionId::TerminalPageSwitcher {
                    let state = owner.read(cx);
                    let route = state.route.route_snapshot();
                    page_switcher(
                        &state.navigation,
                        frame,
                        TERMINAL_PAGE_SWITCHER,
                        &[
                            TerminalSettingsPage::Display,
                            TerminalSettingsPage::Input,
                            TerminalSettingsPage::Local,
                            TerminalSettingsPage::Transfer,
                            TerminalSettingsPage::CommandBar,
                            TerminalSettingsPage::Awareness,
                            TerminalSettingsPage::Logging,
                            TerminalSettingsPage::Highlight,
                        ]
                        .into_iter()
                        .filter(|&page| {
                            state
                                .sections_for(
                                    SettingsTab::Terminal,
                                    SettingsDynamicSectionCounts {
                                        terminal_page: page,
                                        ..frame.counts
                                    },
                                )
                                .iter()
                                .any(|s| *s != SettingsSectionId::TerminalPageSwitcher)
                        })
                        .collect::<Vec<_>>(),
                        route.terminal_page,
                        route.previous_terminal_page,
                        TerminalSettingsPage::label_key,
                        |s: &mut PageState, page, cx| {
                            s.fields.blur();
                            s.select = None;
                            let changed = s.route.set_terminal_page(page);
                            cx.notify();
                            changed
                        },
                        &owner.downgrade(),
                    )
                } else if section == SettingsSectionId::AiPageSwitcher {
                    let state = owner.read(cx);
                    let route = state.route.route_snapshot();
                    page_switcher(
                        &state.navigation,
                        frame,
                        "settings-ai-page-switcher",
                        &[
                            AiSettingsPage::General,
                            AiSettingsPage::Providers,
                            AiSettingsPage::Context,
                            AiSettingsPage::Tools,
                        ]
                        .into_iter()
                        .filter(|&page| {
                            state
                                .sections_for(
                                    SettingsTab::Ai,
                                    SettingsDynamicSectionCounts {
                                        ai_page: page,
                                        ..frame.counts
                                    },
                                )
                                .iter()
                                .any(|s| *s != SettingsSectionId::AiPageSwitcher)
                        })
                        .collect::<Vec<_>>(),
                        route.ai_page,
                        route.previous_ai_page,
                        AiSettingsPage::label_key,
                        |s: &mut PageState, page, cx| {
                            s.fields.blur();
                            s.select = None;
                            let changed = s.route.set_ai_page(page);
                            cx.notify();
                            changed
                        },
                        &owner.downgrade(),
                    )
                } else if section == SettingsSectionId::AiProvidersSettings {
                    ai_providers.clone().into_any_element()
                } else {
                    pages::render_section(
                        &owner,
                        &frame.tokens,
                        dark,
                        section,
                        section_dispatch.clone(),
                        cx,
                    )
                }
            },
        );
        let owner = self.state.clone();
        let picker = self.picker.clone();
        let root = settings_layout(&tokens, has_background, nav, content)
            .id("portable-settings")
            .min_h_0()
            .font_family(ui_font)
            .track_focus(&focus)
            .capture_any_mouse_down({
                let focus = focus.clone();
                let owner = self.state.clone();
                move |event, w, cx| {
                    w.focus(&focus, cx);
                    owner.update(cx, |s, cx| {
                        if s.fields.active.is_some() && !s.fields.hit(event.position) {
                            s.fields.blur();
                            cx.notify();
                        }
                    });
                }
            })
            .capture_key_down(move |event: &KeyDownEvent, window, cx| {
                owner.update(cx, |s, cx| s.key(event, &picker, window, cx));
            })
            .on_mouse_up(MouseButton::Left, {
                let state = self.state.clone();
                move |_, _, cx| state.update(cx, |s, _| s.slider = None)
            });
        let overlays = self.overlays(&tokens, dispatch, cx);
        root.children(overlays).when(ime, |root| {
            let owner = self.state.clone();
            root.child(
                gpui::canvas(
                    |_, _, _| (),
                    move |bounds, _, window, cx| {
                        window.handle_input(
                            &focus,
                            gpui::ElementInputHandler::new(bounds, owner),
                            cx,
                        );
                    },
                )
                .absolute()
                .inset_0(),
            )
        })
    }
}
