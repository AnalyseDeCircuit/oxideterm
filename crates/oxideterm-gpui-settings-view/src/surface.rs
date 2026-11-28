use crate::{SettingsSearchResult, SettingsTab};
use gpui::{
    AnyElement, App, Context, Pixels, Point, Rgba, ScrollHandle, SharedString, Task, WeakEntity,
    Window, div, prelude::*, px, rgb, rgba,
};
use oxideterm_gpui_ui::{
    motion::UserSegmentedControlMotionState, scroll::Scrollbar, virtual_list::tauri_virtual_list,
};
use oxideterm_i18n::I18n;
use oxideterm_settings_model::{
    SETTINGS_SECTION_HEADER_ITEM_COUNT, SettingsDynamicSectionCounts, SettingsSectionId,
};
use oxideterm_theme::ThemeTokens;
use std::{collections::HashMap, rc::Rc};

#[cfg(not(target_family = "wasm"))]
use {
    crate::{TerminalSettingsPage, entity::SettingsWorkspaceEntity},
    gpui::{Entity, Render, Subscription},
    oxideterm_settings_model::{AiSettingsPage, settings_tab_section_at},
};
#[cfg(not(target_family = "wasm"))]
mod host;
pub(crate) mod nav;
#[cfg(not(target_family = "wasm"))]
pub(crate) mod pages;
#[cfg(not(target_family = "wasm"))]
pub use host::{
    McpTargetLabel, SettingsDocumentText, SettingsSurfaceHost, settings_controls_source,
};
#[cfg(not(target_family = "wasm"))]
mod desktop_layout;
mod search_filter;
mod section_cache;

#[cfg(not(target_family = "wasm"))]
pub use desktop_layout::SettingsSectionLayout;
#[cfg(not(target_family = "wasm"))]
pub use pages::{
    KnowledgePageData, PreparedSettingsPage, SettingsPageAction, SettingsPageData,
    SettingsPageFrame, SettingsPageWindowAction, TriggerPageRuntime,
};
pub use section_cache::SettingsSectionList;
mod portable;
pub use crate::known_hosts_page::{PortableKnownHostEntry, PortableKnownHostsSnapshot};
pub use portable::{
    BrowserWallpaperGallery, PortableKnowledgeLibrary, PortableKnownHostsAction,
    PortableProxyProbe, PortableSettingsChanged, PortableSettingsSurface, WallpaperAction,
    WallpaperTile,
};

pub struct SettingsSurfaceFrame {
    pub tokens: ThemeTokens,
    pub i18n: I18n,
    pub has_background: bool,
    pub ui_font: SharedString,
    pub counts: SettingsDynamicSectionCounts,
    pub signatures: Vec<u64>,
    pub divider: Rgba,
    pub groups: Vec<Vec<SettingsTab>>,
}

/// The application supplies data and window operations; the surface owns page layout.
#[cfg(not(target_family = "wasm"))]
pub struct SettingsSurfaceBindings {
    pub frame: Rc<dyn Fn(&App) -> Option<SettingsSurfaceFrame>>,
    pub page: Rc<
        dyn for<'a> Fn(
            SettingsSectionId,
            Option<PreparedSettingsPage>,
            &'a App,
        ) -> Option<SettingsPageFrame<'a>>,
    >,
    pub prepare_page: Rc<dyn Fn(SettingsSectionId, &mut App) -> Option<PreparedSettingsPage>>,
    pub scrolled: Rc<dyn Fn(&mut App)>,
    pub keybinding_note: Rc<dyn Fn(String, u32, &mut App) -> Option<AnyElement>>,
    pub dispatch: Rc<dyn Fn(SettingsSurfaceRequest, &mut Window, &mut App)>,
    pub search_field: Rc<dyn Fn(&str, bool, &App) -> Option<AnyElement>>,
    pub knowledge_available: Rc<dyn Fn(&App) -> bool>,
}

pub enum SettingsSurfaceRequest {
    ToggleSearch,
    ClearSearch,
    SearchResult(SettingsSearchResult),
    OpenNavigationEditor,
    SelectedTab(SettingsTab),
    Tooltip(String, Option<Point<Pixels>>),
}

#[cfg(not(target_family = "wasm"))]
pub struct SettingsSurfaceView {
    settings: Entity<SettingsWorkspaceEntity>,
    bindings: SettingsSurfaceBindings,
    _settings_changed: Subscription,
    navigation: NavigationState,
    search_filter: search_filter::SettingsSearchFilter,
}

#[cfg(not(target_family = "wasm"))]
impl SettingsSurfaceView {
    pub fn follow_knowledge_progress<T: 'static>(
        &self,
        source: &Entity<T>,
        cx: &mut Context<Self>,
    ) {
        cx.observe(source, |surface, _, cx| {
            if surface.settings.read(cx).route_snapshot().active_tab == SettingsTab::Knowledge {
                cx.notify();
            }
        })
        .detach();
    }

    pub fn new(
        settings: Entity<SettingsWorkspaceEntity>,
        bindings: SettingsSurfaceBindings,
        cx: &mut Context<Self>,
    ) -> Self {
        let settings_changed = cx.observe(&settings, |view, settings, cx| {
            let state = settings.read(cx);
            let query = if state.settings_search_open() {
                state.settings_search_query().trim()
            } else {
                ""
            };
            if query != view.search_filter.observed_query() {
                view.queue_search_filter(query.to_owned(), cx);
            }
            cx.notify();
        });
        Self {
            settings,
            bindings,
            _settings_changed: settings_changed,
            navigation: Default::default(),
            search_filter: Default::default(),
        }
    }
}

#[derive(Default)]
struct NavigationState {
    nav_scroll: ScrollHandle,
    search_scroll: ScrollHandle,
    motion: UserSegmentedControlMotionState,
    tasks: HashMap<&'static str, Task<()>>,
}

trait SettingsRouteOwner: Sized + 'static {
    fn navigation_mut(&mut self) -> &mut NavigationState;
    fn active_tab(&self, cx: &App) -> SettingsTab;
    fn select_tab(&mut self, tab: SettingsTab, window: &mut Window, cx: &mut Context<Self>);

    fn animate_route(
        &mut self,
        id: &'static str,
        tokens: &ThemeTokens,
        target_index: usize,
        offset: Option<f32>,
        cx: &mut Context<Self>,
    ) {
        let navigation = self.navigation_mut();
        let Some(motion) = oxideterm_gpui_ui::segmented_control_motion(tokens) else {
            navigation.motion.clear(id);
            navigation.tasks.remove(id);
            return;
        };
        let generation = navigation
            .motion
            .begin_with_vertical_offset(id, target_index, offset);
        let task = cx.spawn(async move |view, cx| {
            cx.background_executor().timer(motion.duration).await;
            let _ = view.update(cx, |view, cx| {
                if view.navigation_mut().motion.finish(id, generation) {
                    cx.notify();
                }
            });
        });
        navigation.tasks.insert(id, task);
    }
}

#[cfg(not(target_family = "wasm"))]
impl SettingsRouteOwner for SettingsSurfaceView {
    fn navigation_mut(&mut self) -> &mut NavigationState {
        &mut self.navigation
    }

    fn active_tab(&self, cx: &App) -> SettingsTab {
        self.settings.read(cx).route_snapshot().active_tab
    }

    fn select_tab(&mut self, tab: SettingsTab, window: &mut Window, cx: &mut Context<Self>) {
        self.settings
            .update(cx, |settings, cx| settings.set_active_tab(tab, cx));
        (self.bindings.dispatch)(SettingsSurfaceRequest::SelectedTab(tab), window, cx);
    }
}

const TERMINAL_PAGE_SWITCHER: &str = "terminal-settings-page-switcher";

fn page_switcher<T: SettingsRouteOwner, Page: Copy + Eq + 'static>(
    navigation: &NavigationState,
    frame: &SettingsSurfaceFrame,
    id: &'static str,
    pages: &[Page],
    current: Page,
    previous: Page,
    label: impl Fn(Page) -> &'static str,
    set_page: impl Fn(&mut T, Page, &mut Context<T>) -> bool + Copy + 'static,
    view: &WeakEntity<T>,
) -> AnyElement {
    let active_index = pages.iter().position(|page| *page == current).unwrap_or(0);
    let previous_index = pages
        .iter()
        .position(|page| *page == previous)
        .unwrap_or(active_index);
    let tokens = frame.tokens;
    let items = pages
        .iter()
        .copied()
        .enumerate()
        .map(|(index, page)| {
            let view = view.clone();
            let title = label(page);
            oxideterm_gpui_ui::segmented_control_item(&tokens, frame.i18n.t(title), page == current)
                .debug_selector(move || title.into())
                .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                    let _ = view.update(cx, |view, cx| {
                        if set_page(view, page, cx) {
                            view.animate_route(id, &tokens, index, None, cx);
                        }
                    });
                })
                .into_any_element()
        })
        .collect::<Vec<_>>();
    oxideterm_gpui_ui::segmented_control(
        &tokens,
        id,
        oxideterm_gpui_ui::SegmentedControlOptions::new(active_index, previous_index, pages.len())
            .user_transition_active(navigation.motion.is_active_for(id, active_index))
            .has_background_image(frame.has_background),
        items,
    )
    .into_any_element()
}

#[cfg(not(target_family = "wasm"))]
impl Render for SettingsSurfaceView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(frame) = (self.bindings.frame)(cx) else {
            return div().into_any_element();
        };
        let settings = self.settings.read(cx);
        let tab = settings.route_snapshot().active_tab;
        settings
            .section_list
            .sync(tab, frame.counts, &frame.signatures);
        let state = settings.section_list.state().clone();
        let cached;
        let search = if settings.settings_search_open() {
            let query = settings.settings_search_query();
            cached = self.search_filter.results(&frame.i18n);
            Some(nav::SearchPanel {
                query,
                input: (self.bindings.search_field)(
                    query,
                    settings.settings_entity_focused_input()
                        == Some(crate::SettingsInput::SettingsSearch),
                    cx,
                ),
                results: cached
                    .iter()
                    .filter(|result| {
                        result.section != SettingsSectionId::KnowledgeDocuments
                            || (self.bindings.knowledge_available)(cx)
                    })
                    .collect(),
            })
        } else {
            None
        };
        let navigation = nav::Sidebar {
            state: &self.navigation,
            owner: cx.entity().downgrade(),
            active_tab: tab,
            dispatch: self.bindings.dispatch.clone(),
            desktop_toolbar: true,
        }
        .render(&frame, search);
        let page = self.bindings.page.clone();
        let prepare_page = self.bindings.prepare_page.clone();
        let settings_owner = self.settings.clone();
        let scrolled = self.bindings.scrolled.clone();
        let keybinding_note = self.bindings.keybinding_note.clone();
        let view = cx.entity().downgrade();
        let tokens = frame.tokens;
        let has_background = frame.has_background;
        let counts = frame.counts;
        let content = settings_content(
            frame,
            tab,
            state,
            move |index| settings_tab_section_at(tab, index, counts),
            move |s, color, cx| keybinding_note(s, color, cx),
            move |cx| scrolled(cx),
            move |frame, id, cx| match id {
                SettingsSectionId::TerminalPageSwitcher | SettingsSectionId::AiPageSwitcher => {
                    let Some(owner) = view.upgrade() else {
                        return div().into_any_element();
                    };
                    let surface = owner.read(cx);
                    let route = surface.settings.read(cx).route_snapshot();
                    if id == SettingsSectionId::TerminalPageSwitcher {
                        page_switcher(
                            &surface.navigation,
                            frame,
                            TERMINAL_PAGE_SWITCHER,
                            TerminalSettingsPage::all(),
                            route.terminal_page,
                            route.previous_terminal_page,
                            TerminalSettingsPage::label_key,
                            |view, page, cx| {
                                view.settings
                                    .update(cx, |settings, cx| settings.set_terminal_page(page, cx))
                            },
                            &view,
                        )
                    } else {
                        page_switcher(
                            &surface.navigation,
                            frame,
                            "ai-settings-page-switcher",
                            AiSettingsPage::all(),
                            route.ai_page,
                            route.previous_ai_page,
                            AiSettingsPage::label_key,
                            |view, page, cx| {
                                view.settings
                                    .update(cx, |settings, cx| settings.set_ai_page(page, cx))
                            },
                            &view,
                        )
                    }
                }
                id => {
                    let prepared = prepare_page(id, cx);
                    if let Some(page) = page(id, prepared, cx) {
                        page.render(&settings_owner, id, cx)
                    } else {
                        div().into_any_element()
                    }
                }
            },
        );
        settings_layout(&tokens, has_background, navigation, content).into_any_element()
    }
}

fn settings_content(
    frame: SettingsSurfaceFrame,
    tab: SettingsTab,
    state: gpui::ListState,
    section_at: impl Fn(usize) -> Option<SettingsSectionId> + 'static,
    keybinding_note: impl Fn(String, u32, &mut App) -> Option<AnyElement> + 'static,
    scrolled: impl Fn(&mut App) + 'static,
    mut render_section: impl FnMut(&SettingsSurfaceFrame, SettingsSectionId, &mut App) -> AnyElement
    + 'static,
) -> AnyElement {
    let tokens = frame.tokens;
    let count = state.item_count();
    let scrollbar = Scrollbar::for_list(&state).id("settings-content-scrollbar");
    let rows = tauri_virtual_list(
        state,
        SettingsSectionList::spec(tab),
        move |index, _, cx| {
            let section = index
                .checked_sub(SETTINGS_SECTION_HEADER_ITEM_COUNT)
                .and_then(&section_at);
            let row = match section {
                Some(id) => render_section(&frame, id, cx),
                None if index == 0 => page_header(&frame, tab, &keybinding_note, cx),
                None => div().into_any_element(),
            };
            let padding = tokens.metrics.settings_content_padding;
            div()
                .w_full()
                .min_w(px(0.0))
                .debug_selector(move || format!("{section:?}"))
                .px(px(padding))
                .pb(px(if index + 1 == count {
                    padding
                } else {
                    tokens.metrics.settings_page_gap
                }))
                .when(index == 0, |item| item.pt(px(padding)))
                .child(row)
                .into_any_element()
        },
    );
    let body = div()
        .id("settings-content-scroll")
        .size_full()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .on_scroll(move |_, _, cx| scrolled(cx))
        .relative()
        .child(rows)
        .child(scrollbar);
    div()
        .flex_1()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .relative()
        .child(oxideterm_gpui_ui::motion::fade_in(
            &tokens,
            SharedString::from(format!("settings-page-{tab:?}")),
            body,
            oxideterm_gpui_ui::motion::MotionDuration::Micro,
        ))
        .into_any_element()
}

fn settings_layout(
    tokens: &ThemeTokens,
    has_background: bool,
    navigation: AnyElement,
    content: AnyElement,
) -> gpui::Div {
    div()
        .size_full()
        .relative()
        .flex()
        .flex_row()
        .bg(if has_background {
            rgba(0x00000000)
        } else {
            rgb(tokens.ui.bg)
        })
        .text_color(rgb(tokens.ui.text))
        .child(navigation)
        .child(content)
}

fn page_header(
    frame: &SettingsSurfaceFrame,
    tab: SettingsTab,
    keybinding_note: &dyn Fn(String, u32, &mut App) -> Option<AnyElement>,
    cx: &mut App,
) -> AnyElement {
    let tokens = frame.tokens;
    let size = tokens.metrics.ui_text_2xl;
    let description_size = tokens.metrics.ui_text_sm;
    let header = div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(tokens.spacing.two))
        .font_family(frame.ui_font.clone())
        .child(
            div()
                .w_full()
                .min_w(px(0.0))
                .text_size(px(size))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(rgb(tokens.ui.text_heading))
                .line_height(px(size + 6.0))
                .child(frame.i18n.t(tab.title_key())),
        )
        .child(
            div()
                .w_full()
                .min_w(px(0.0))
                .text_size(px(description_size))
                .text_color(rgb(tokens.ui.text_muted))
                .line_height(px(description_size + 6.0))
                .child(frame.i18n.t(tab.description_key())),
        )
        .when(tab == SettingsTab::Keybindings, |header| {
            let note = keybinding_note(
                frame.i18n.t("settings_view.keybindings.intl_keyboard_note"),
                tokens.ui.text_muted,
                cx,
            );
            header.child(
                div()
                    .mt(px(2.0))
                    .w_full()
                    .min_w(px(0.0))
                    .text_size(px(tokens.metrics.ui_text_xs))
                    .text_color(rgba((tokens.ui.text_muted << 8) | 0xb3))
                    .line_height(px((tokens.metrics.ui_text_xs + 4.0).max(16.0)))
                    .children(note),
            )
        });
    div()
        .w_full()
        .relative()
        .flex()
        .flex_col()
        .gap(px(tokens.spacing.three + tokens.spacing.one))
        .child(header)
        .child(
            div()
                .h(px(1.0))
                .w_full()
                .bg(rgba((tokens.ui.border << 8) | 0x66)),
        )
        .into_any_element()
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::*;
    use gpui::AppContext;
    use oxideterm_i18n::Locale;
    use std::time::Duration;

    #[gpui::test]
    fn settings_search_filters_only_the_latest_query_after_the_delay(
        cx: &mut gpui::TestAppContext,
    ) {
        let settings = cx.new(SettingsWorkspaceEntity::new);
        let surface = cx.new(|cx| {
            SettingsSurfaceView::new(
                settings.clone(),
                SettingsSurfaceBindings {
                    frame: Rc::new(|_| None),
                    page: Rc::new(|_, _, _| None),
                    prepare_page: Rc::new(|_, _| None),
                    scrolled: Rc::new(|_| {}),
                    keybinding_note: Rc::new(|_, _, _| None),
                    dispatch: Rc::new(|_, _, _| {}),
                    search_field: Rc::new(|_, _, _| None),
                    knowledge_available: Rc::new(|_| false),
                },
                cx,
            )
        });
        let input = crate::SettingsInput::SettingsSearch;
        settings.update(cx, |settings, cx| {
            settings.open_settings_search(cx);
            assert!(settings.focus_settings_entity_input(input, cx));
            assert!(settings.replace_settings_entity_input(input, None, "Semantic Search", cx));
        });
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(80));
        settings.update(cx, |settings, cx| {
            assert!(settings.replace_settings_entity_input(
                input,
                Some(0.."Semantic Search".len()),
                "MCP Servers",
                cx,
            ));
        });
        cx.run_until_parked();
        let locale = I18n::new(Locale::En);
        let first = |cx: &mut gpui::TestAppContext| {
            surface.read_with(cx, |view, _| {
                view.search_filter
                    .results(&locale)
                    .first()
                    .map(|result| result.section)
            })
        };
        cx.executor().advance_clock(Duration::from_millis(50));
        cx.run_until_parked();
        assert_eq!(first(cx), None);
        cx.executor().advance_clock(Duration::from_millis(80));
        cx.run_until_parked();
        assert_eq!(first(cx), Some(SettingsSectionId::AiToolsMcpServers));

        settings.update(cx, |settings, cx| {
            settings.clear_settings_search_query(cx);
        });
        assert_eq!(first(cx), None);
    }

    #[gpui::test]
    fn terminal_subpage_click_replaces_rows_without_dispatching_to_the_host(
        cx: &mut gpui::TestAppContext,
    ) {
        let settings = cx.new(SettingsWorkspaceEntity::new);
        settings.update(cx, |state, cx| {
            state.set_active_tab(SettingsTab::Terminal, cx)
        });
        let (_, cx) = cx.add_window_view(|_, cx| {
            let state = settings.clone();
            SettingsSurfaceView::new(
                settings.clone(),
                SettingsSurfaceBindings {
                    frame: Rc::new(move |cx| {
                        let route = state.read(cx).route_snapshot();
                        let counts = SettingsDynamicSectionCounts {
                            terminal_page: route.terminal_page,
                            ai_page: route.ai_page,
                            visible_keybinding_scope_count: 0,
                            knowledge_has_error: false,
                            knowledge_has_selected_collection: false,
                        };
                        Some(SettingsSurfaceFrame {
                            tokens: oxideterm_theme::default_tokens(),
                            i18n: I18n::new(Locale::En),
                            has_background: false,
                            ui_font: ".SystemUIFont".into(),
                            counts,
                            signatures: (0
                                ..oxideterm_settings_model::settings_section_list_item_count(
                                    route.active_tab,
                                    counts,
                                ) as u64)
                                .collect(),
                            divider: rgb(0xcccccc),
                            groups: vec![vec![SettingsTab::Terminal]],
                        })
                    }),
                    page: Rc::new(|_, _, _| None),
                    prepare_page: Rc::new(|_, _| None),
                    scrolled: Rc::new(|_| {}),
                    keybinding_note: Rc::new(|_, _, _| None),
                    dispatch: Rc::new(|request, _, _| {
                        assert!(matches!(request, SettingsSurfaceRequest::Tooltip(..)));
                    }),
                    search_field: Rc::new(|_, _, _| None),
                    knowledge_available: Rc::new(|_| false),
                },
                cx,
            )
        });
        cx.simulate_resize(gpui::size(px(1280.0), px(750.0)));
        cx.update(|window, app| window.draw(app).clear(app));
        assert!(cx.debug_bounds("Some(TerminalDisplayFont)").is_some());
        let logging = cx
            .debug_bounds(TerminalSettingsPage::Logging.label_key())
            .unwrap();
        cx.simulate_mouse_down(
            logging.center(),
            gpui::MouseButton::Left,
            gpui::Modifiers::default(),
        );
        cx.update(|window, app| window.draw(app).clear(app));
        assert_eq!(
            settings.read_with(cx, |state, _| state.route_snapshot().terminal_page),
            TerminalSettingsPage::Logging
        );
        assert!(cx.debug_bounds("Some(TerminalLoggingSession)").is_some());
        assert!(cx.debug_bounds("Some(TerminalDisplayFont)").is_none());
    }
}
