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
use oxideterm_gpui_ui::{
    select::{OverlayAnchor, SelectAnchorId},
    virtual_list::tauri_virtual_list,
};
use oxideterm_i18n::{I18n, Locale};
use oxideterm_settings::{Language, PersistedSettings};
use oxideterm_settings_model::{
    AiSettingsPage, SettingsDynamicSectionCounts, SettingsSectionId, ThemeEditorState,
    settings_tab_sections,
};
use oxideterm_theme::ThemeTokens;

use super::{
    SettingsSectionList, SettingsSurfaceFrame, nav, page_header,
    search_filter::SettingsSearchFilter,
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
    _observe: Subscription,
    _edits: Subscription,
}

impl EventEmitter<PortableSettingsChanged> for PortableSettingsSurface {}
impl EventEmitter<PortableSettingsChanged> for PageState {}

fn supported(tab: SettingsTab, section: SettingsSectionId) -> bool {
    use SettingsSectionId::*;
    match tab {
        SettingsTab::General => matches!(
            section,
            GeneralLanguage | GeneralExternalConnectionUris | GeneralWindowBehavior
        ),
        SettingsTab::Connections => section == ConnectionsKnownHosts,
        SettingsTab::Network => matches!(section, NetworkSharedProxy | NetworkRouting),
        SettingsTab::Help => matches!(
            section,
            HelpVersion | HelpDiagnostics | HelpTechStack | HelpResources | HelpSafety | HelpLegal
        ),
        SettingsTab::Appearance => matches!(section, AppearanceTheme | AppearanceLayout),
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
        ),
        SettingsTab::Sftp => matches!(section, SftpOpening | SftpPerformance | SftpConflict),
        SettingsTab::Ide => matches!(
            section,
            IdeEditing | IdeTypography | IdeAgent | IdeAgentPrivacy
        ),
        SettingsTab::Keybindings => matches!(section, KeybindingsToolbar | KeybindingsScope(_)),
        _ => false,
    }
}

struct PageState {
    settings: PersistedSettings,
    catalog: Vec<ActionDefinition>,
    route: SettingsRouteState,
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
    document: Option<(
        bool,
        oxideterm_gpui_markdown::MarkdownVirtualListScrollHandle,
    )>,
    shortcuts: KeybindingState,
    reset_open: bool,
    i18n: I18n,
}

impl PortableSettingsSurface {
    pub fn new(
        settings: PersistedSettings,
        actions: Vec<ActionDefinition>,
        cx: &mut Context<Self>,
    ) -> Self {
        let state = cx.new(|cx| PageState {
            i18n: I18n::new(locale(settings.general.language)),
            settings,
            catalog: actions,
            route: SettingsRouteState::default(),
            list: Default::default(),
            target: Cell::new(None),
            query: String::new(),
            search: SettingsSearchFilter::new(|i18n, query| {
                crate::search::filtered_results(i18n, query, supported, false)
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
            probe: Default::default(),
            document: None,
            shortcuts: Default::default(),
            reset_open: false,
        });
        let picker = actions::theme_picker(&state, cx);
        Self {
            _observe: cx.observe(&state, |_, _, cx| cx.notify()),
            _edits: cx.subscribe(&state, |_, _, _: &PortableSettingsChanged, cx| {
                cx.emit(PortableSettingsChanged)
            }),
            state,
            picker,
        }
    }

    pub fn settings<'a>(&self, cx: &'a App) -> &'a PersistedSettings {
        &self.state.read(cx).settings
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
        if tab != SettingsTab::Network {
            self.clear_proxy_password();
        }
        self.route.set_active_tab(tab);
        if let Some(page) = result.and_then(|r| r.terminal_page) {
            self.route.set_terminal_page(page);
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
                has_background: false,
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

    fn frame(&self, dark: bool) -> (SettingsSurfaceFrame, Vec<SettingsSectionId>) {
        let route = self.route.route_snapshot();
        let counts = SettingsDynamicSectionCounts {
            terminal_page: route.terminal_page,
            ai_page: AiSettingsPage::General,
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
        };
        let sections_for = |tab| {
            let mut sections = settings_tab_sections(tab, counts);
            // A gateway can edit desktop preferences irrespective of its own platform.
            if tab == SettingsTab::General
                && !sections.contains(&SettingsSectionId::GeneralWindowBehavior)
            {
                sections.push(SettingsSectionId::GeneralWindowBehavior);
            }
            sections
        };
        let tabs = SettingsTab::all()
            .iter()
            .copied()
            .filter(|&tab| sections_for(tab).into_iter().any(|s| supported(tab, s)))
            .collect();
        let sections: Vec<_> = sections_for(route.active_tab)
            .into_iter()
            .filter(|&s| supported(route.active_tab, s))
            .collect();
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
                has_background: false,
                ui_font: oxideterm_gpui_ui::typography::tauri_ui_font_family(
                    &self.settings.appearance.ui_font_family,
                ),
                divider: rgb(tokens.ui.border),
                groups: vec![tabs],
            },
            sections,
        )
    }
}

impl Render for PortableSettingsSurface {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let dark = system_dark(window);
        let (frame, sections) = state.frame(dark);
        let tab = state.route.route_snapshot().active_tab;
        state.list.sync(tab, frame.counts, &frame.signatures);
        if let Some(target) = state.target.take()
            && let Some(index) = sections.iter().position(|s| *s == target)
        {
            state.list.state().scroll_to(gpui::ListOffset {
                item_ix: index + 1,
                offset_in_item: px(0.0),
            });
        }
        let tokens = frame.tokens;
        let dispatch = actions::window_dispatch(self.state.downgrade(), self.picker.downgrade());
        let nav = self.navigation(&frame, dispatch.clone(), cx);
        let list = state.list.state().clone();
        let owner = self.state.clone();
        let focus = state.focus.clone();
        let ime = state.fields.active.is_some();
        let ui_font = frame.ui_font.clone();
        let rows = tauri_virtual_list(list, SettingsSectionList::spec(tab), {
            let dispatch = dispatch.clone();
            move |index, _, cx| {
                let pad = px(tokens.metrics.settings_content_padding);
                let row = if index == 0 {
                    page_header(
                        &frame,
                        tab,
                        &|s, _, _| Some(div().child(s).into_any_element()),
                        cx,
                    )
                } else {
                    pages::render_section(
                        &owner,
                        &tokens,
                        dark,
                        sections[index - 1],
                        dispatch.clone(),
                        cx,
                    )
                };
                div()
                    .w_full()
                    .min_w_0()
                    .px(pad)
                    .pb(px(tokens.metrics.settings_page_gap))
                    .when(index == 0, |row| row.pt(pad))
                    .child(row)
                    .into_any_element()
            }
        });
        let owner = self.state.clone();
        let picker = self.picker.clone();
        let root = div()
            .id("portable-settings")
            .size_full()
            .relative()
            .flex()
            .min_h_0()
            .font_family(ui_font)
            .bg(rgb(tokens.ui.bg))
            .text_color(rgb(tokens.ui.text))
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
            })
            .child(nav)
            .child(
                div()
                    .id("portable-settings-content")
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .on_scroll({
                        let owner = self.state.clone();
                        move |_, _, cx| {
                            owner.update(cx, |s, cx| {
                                s.anchors.clear();
                                if s.select.take().is_some() {
                                    cx.notify();
                                }
                            })
                        }
                    })
                    .child(rows),
            );
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

impl PortableSettingsSurface {
    fn navigation(
        &self,
        frame: &SettingsSurfaceFrame,
        dispatch: crate::components::SettingsWindowDispatch,
        cx: &App,
    ) -> AnyElement {
        let state = self.state.read(cx);
        let controls = state.controls(&frame.tokens, dispatch.clone());
        let field = controls.navigation_search_field(
            SettingsImeSource::frame(state, SettingsInput::SettingsSearch, &state.query, cx),
            dispatch,
        );
        let mut nav = div()
            .id("portable-settings-nav")
            .w(px(frame.tokens.metrics.settings_nav_width))
            .h_full()
            .flex_none()
            .overflow_y_scroll()
            .bg(rgb(frame.tokens.ui.bg_panel))
            .p_2()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_color(rgb(frame.tokens.ui.text_heading))
                    .child(frame.i18n.t("settings_view.title")),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .border_1()
                    .border_color(rgb(frame.tokens.ui.border))
                    .rounded(px(frame.tokens.radii.md))
                    .px_2()
                    .h(px(32.0))
                    .child(nav::render_icon(
                        "lucide/search.svg",
                        15.0,
                        rgb(frame.tokens.ui.text_muted),
                    ))
                    .child(field),
            );
        if !state.query.trim().is_empty() && state.search_open {
            let results = state.search.results(&state.i18n);
            if results.is_empty() {
                nav = nav.child(frame.i18n.t("settings_view.search.no_results"));
            }
            for result in results.iter().cloned() {
                let owner = self.state.clone();
                nav = nav.child(
                    div()
                        .cursor_pointer()
                        .p_2()
                        .child(result.label.clone())
                        .child(
                            div()
                                .text_size(px(frame.tokens.metrics.ui_text_xs))
                                .text_color(rgb(frame.tokens.ui.text_muted))
                                .child(result.breadcrumb.clone()),
                        )
                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                            owner.update(cx, |s, cx| s.navigate(result.tab, Some(&result), cx));
                        }),
                );
            }
        } else {
            for &tab in frame.groups.iter().flatten() {
                let active = state.route.route_snapshot().active_tab == tab;
                let owner = self.state.clone();
                let selected = active.then(|| {
                    div()
                        .absolute()
                        .inset_0()
                        .rounded(px(frame.tokens.radii.md))
                        .bg(rgb(frame.tokens.ui.bg_active))
                        .into_any_element()
                });
                nav = nav.child(
                    nav::nav_row(&frame.tokens, &frame.i18n, tab, active, selected)
                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                            owner.update(cx, |s, cx| s.navigate(tab, None, cx))
                        }),
                );
            }
        }
        nav.into_any_element()
    }
}
