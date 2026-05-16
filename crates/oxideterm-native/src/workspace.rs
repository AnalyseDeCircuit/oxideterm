use std::collections::HashMap;

use anyhow::Result;
use gpui::{
    AnyElement, App, Context, CursorStyle, FocusHandle, Focusable, IntoElement, KeyDownEvent,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Render, Rgba,
    SharedString, Styled, Window, div, prelude::*, px, relative, rgb, rgba, svg,
};
use oxideterm_gpui_terminal::TerminalPane;
use oxideterm_i18n::{I18n, Locale};
use oxideterm_theme::{ThemeTokens, default_tokens};
use oxideterm_workspace::{
    MAX_PANES_PER_TAB, PaneId, PaneNode, SplitDirection, Tab, TabId, TabKind, TerminalSessionId,
    adjusted_split_sizes, balanced_sizes,
};

use crate::assets::LucideIcon;
use crate::{
    ClosePane, CloseSearch, CloseTab, Copy, Find, FindNext, FindPrev, GoToTab1, GoToTab2, GoToTab3,
    GoToTab4, GoToTab5, GoToTab6, GoToTab7, GoToTab8, GoToTab9, NewTerminal, NextTab, Paste,
    PrevTab, SplitHorizontal, SplitVertical, SwitchLocaleChinese, SwitchLocaleEnglish,
};

#[derive(Clone)]
struct SplitDrag {
    group_id: PaneId,
    handle_index: usize,
    direction: SplitDirection,
    start_position: gpui::Point<Pixels>,
    start_sizes: Vec<f32>,
}

#[derive(Default)]
struct SearchBarState {
    visible: bool,
    query: String,
    active_match: Option<usize>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum SidebarSection {
    Sessions,
    Connections,
    Terminal,
    Activity,
    Network,
    Extensions,
    Assistant,
    Automation,
    Workspace,
    Files,
    Monitor,
    Notifications,
    Settings,
}

pub(crate) struct WorkspaceApp {
    focus_handle: FocusHandle,
    tabs: Vec<Tab>,
    active_tab_id: Option<TabId>,
    panes: HashMap<PaneId, gpui::Entity<TerminalPane>>,
    next_tab_id: u64,
    next_pane_id: u64,
    next_session_id: u64,
    search: SearchBarState,
    split_drag: Option<SplitDrag>,
    sidebar_resizing: bool,
    sidebar_collapsed: bool,
    sidebar_width: f32,
    needs_active_pane_focus: bool,
    active_sidebar_section: SidebarSection,
    i18n: I18n,
    tokens: ThemeTokens,
}

impl WorkspaceApp {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Result<Self> {
        let focus_handle = cx.focus_handle();
        let mut workspace = Self {
            focus_handle,
            tabs: Vec::new(),
            active_tab_id: None,
            panes: HashMap::new(),
            next_tab_id: 1,
            next_pane_id: 1,
            next_session_id: 1,
            search: SearchBarState::default(),
            split_drag: None,
            sidebar_resizing: false,
            sidebar_collapsed: false,
            sidebar_width: default_tokens().metrics.sidebar_default_width,
            needs_active_pane_focus: false,
            active_sidebar_section: SidebarSection::Sessions,
            i18n: I18n::default(),
            tokens: default_tokens(),
        };
        workspace.create_local_terminal_tab(window, cx)?;
        Ok(workspace)
    }

    fn create_local_terminal_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let tab_id = self.alloc_tab_id();
        let pane_id = self.alloc_pane_id();
        let session_id = self.alloc_session_id();
        let pane =
            cx.new(|cx| TerminalPane::new(window, cx).expect("failed to initialize terminal pane"));

        self.panes.insert(pane_id, pane.clone());
        self.tabs.push(Tab {
            id: tab_id,
            kind: TabKind::LocalTerminal,
            title: self.i18n.t("terminal.local_terminal"),
            root_pane: PaneNode::leaf(pane_id, session_id),
            active_pane_id: pane_id,
        });
        self.active_tab_id = Some(tab_id);
        self.needs_active_pane_focus = true;
        pane.read(cx).focus(window);
        cx.notify();
        Ok(())
    }

    fn alloc_tab_id(&mut self) -> TabId {
        let id = TabId(self.next_tab_id);
        self.next_tab_id += 1;
        id
    }

    fn alloc_pane_id(&mut self) -> PaneId {
        let id = PaneId(self.next_pane_id);
        self.next_pane_id += 1;
        id
    }

    fn alloc_session_id(&mut self) -> TerminalSessionId {
        let id = TerminalSessionId(self.next_session_id);
        self.next_session_id += 1;
        id
    }

    fn active_tab_index(&self) -> Option<usize> {
        let active = self.active_tab_id?;
        self.tabs.iter().position(|tab| tab.id == active)
    }

    fn active_tab(&self) -> Option<&Tab> {
        self.active_tab_index()
            .and_then(|index| self.tabs.get(index))
    }

    fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        let index = self.active_tab_index()?;
        self.tabs.get_mut(index)
    }

    fn active_pane_id(&self) -> Option<PaneId> {
        self.active_tab().map(|tab| tab.active_pane_id)
    }

    fn active_pane(&self) -> Option<gpui::Entity<TerminalPane>> {
        self.active_pane_id()
            .and_then(|pane_id| self.panes.get(&pane_id).cloned())
    }

    fn set_active_tab(&mut self, tab_id: TabId, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.iter().any(|tab| tab.id == tab_id) {
            self.active_tab_id = Some(tab_id);
            self.needs_active_pane_focus = true;
            self.focus_active_pane(window, cx);
            cx.notify();
        }
    }

    fn focus_active_pane(&self, window: &mut Window, cx: &App) {
        if let Some(pane) = self.active_pane() {
            pane.read(cx).focus(window);
        } else {
            window.focus(&self.focus_handle);
        }
    }

    fn close_active_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.active_tab_index() else {
            return;
        };
        let tab = self.tabs.remove(index);
        let mut pane_ids = Vec::new();
        tab.root_pane.collect_pane_ids(&mut pane_ids);
        for pane_id in pane_ids {
            if let Some(pane) = self.panes.remove(&pane_id) {
                let _ = pane.update(cx, |pane, _cx| pane.shutdown());
            }
        }

        self.active_tab_id = if self.tabs.is_empty() {
            None
        } else {
            Some(self.tabs[index.saturating_sub(1).min(self.tabs.len() - 1)].id)
        };
        self.needs_active_pane_focus = self.active_tab_id.is_some();
        self.focus_active_pane(window, cx);
        cx.notify();
    }

    fn next_tab(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.is_empty() {
            return;
        }
        let current = self.active_tab_index().unwrap_or(0);
        let next = if forward {
            (current + 1) % self.tabs.len()
        } else if current == 0 {
            self.tabs.len() - 1
        } else {
            current - 1
        };
        self.active_tab_id = Some(self.tabs[next].id);
        self.needs_active_pane_focus = true;
        self.focus_active_pane(window, cx);
        cx.notify();
    }

    fn go_to_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.tabs.get(index) {
            self.active_tab_id = Some(tab.id);
            self.needs_active_pane_focus = true;
            self.focus_active_pane(window, cx);
            cx.notify();
        }
    }

    fn split_active_pane(
        &mut self,
        direction: SplitDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(active_index) = self.active_tab_index() else {
            return;
        };
        if self.tabs[active_index].root_pane.pane_count() >= MAX_PANES_PER_TAB {
            return;
        }

        let group_id = self.alloc_pane_id();
        let pane_id = self.alloc_pane_id();
        let session_id = self.alloc_session_id();
        let pane = cx.new(|cx| {
            TerminalPane::new(window, cx).expect("failed to initialize split terminal pane")
        });

        let tab = &mut self.tabs[active_index];
        if tab
            .root_pane
            .split_active(tab.active_pane_id, group_id, direction, pane_id, session_id)
        {
            tab.active_pane_id = pane_id;
            self.panes.insert(pane_id, pane.clone());
            self.needs_active_pane_focus = true;
            pane.read(cx).focus(window);
            cx.notify();
        } else {
            let _ = pane.update(cx, |pane, _cx| pane.shutdown());
        }
    }

    fn close_active_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(active_index) = self.active_tab_index() else {
            return;
        };
        let active_pane_id = self.tabs[active_index].active_pane_id;
        if self.tabs[active_index].root_pane.pane_count() <= 1 {
            return;
        }

        if let Some(pane) = self.panes.remove(&active_pane_id) {
            let _ = pane.update(cx, |pane, _cx| pane.shutdown());
        }

        let tab = &mut self.tabs[active_index];
        if let Some(next_active) = tab.root_pane.close_pane(active_pane_id) {
            if let Some(replacement) = tab.root_pane.single_child_replacement() {
                tab.root_pane = replacement;
            }
            tab.active_pane_id = next_active;
            self.needs_active_pane_focus = true;
            self.focus_active_pane(window, cx);
            cx.notify();
        }
    }

    fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search.visible = true;
        window.focus(&self.focus_handle);
        if let Some(pane) = self.active_pane() {
            let query = (!self.search.query.is_empty()).then(|| self.search.query.clone());
            let _ = pane.update(cx, |pane, cx| {
                pane.set_search_query(query, self.search.active_match, cx);
            });
        }
        cx.notify();
    }

    fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search.visible = false;
        self.search.active_match = None;
        if let Some(pane) = self.active_pane() {
            let _ = pane.update(cx, |pane, cx| pane.set_search_query(None, None, cx));
        }
        self.focus_active_pane(window, cx);
        cx.notify();
    }

    fn update_search_query(&mut self, cx: &mut Context<Self>) {
        let query = (!self.search.query.is_empty()).then(|| self.search.query.clone());
        self.search.active_match = query.as_ref().map(|_| 0);
        if let Some(pane) = self.active_pane() {
            let _ = pane.update(cx, |pane, cx| {
                pane.set_search_query(query, self.search.active_match, cx);
            });
        }
        cx.notify();
    }

    fn search_next(&mut self, forward: bool, cx: &mut Context<Self>) {
        if let Some(pane) = self.active_pane() {
            let _ = pane.update(cx, |pane, cx| {
                pane.select_next_search_result(forward, cx);
            });
        }
    }

    fn copy(&mut self, cx: &mut Context<Self>) {
        if let Some(pane) = self.active_pane() {
            let _ = pane.update(cx, |pane, cx| pane.copy_to_clipboard(cx));
        }
    }

    fn paste(&mut self, cx: &mut Context<Self>) {
        if let Some(pane) = self.active_pane() {
            let _ = pane.update(cx, |pane, cx| pane.paste_from_clipboard(cx));
        }
    }

    fn handle_workspace_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;

        if self.search.visible && !modifiers.platform {
            match key {
                "escape" => self.close_search(window, cx),
                "enter" => self.search_next(!modifiers.shift, cx),
                "backspace" => {
                    self.search.query.pop();
                    self.update_search_query(cx);
                }
                "space" => {
                    self.search.query.push(' ');
                    self.update_search_query(cx);
                }
                key if key.chars().count() == 1 && !modifiers.control && !modifiers.alt => {
                    self.search.query.push_str(key);
                    self.update_search_query(cx);
                }
                _ => {}
            }
            return;
        }
    }

    fn start_split_drag(
        &mut self,
        group_id: PaneId,
        handle_index: usize,
        direction: SplitDirection,
        sizes: &[f32],
        event: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        self.split_drag = Some(SplitDrag {
            group_id,
            handle_index,
            direction,
            start_position: event.position,
            start_sizes: sizes.to_vec(),
        });
        cx.notify();
    }

    fn update_split_drag(
        &mut self,
        event: &MouseMoveEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = self.split_drag.clone() else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            return;
        }
        let viewport = window.viewport_size();
        let delta_fraction = match drag.direction {
            SplitDirection::Horizontal => {
                f32::from(event.position.x - drag.start_position.x)
                    / f32::from(viewport.width).max(1.0)
                    * 100.0
            }
            SplitDirection::Vertical => {
                f32::from(event.position.y - drag.start_position.y)
                    / f32::from(viewport.height).max(1.0)
                    * 100.0
            }
        };
        let next_sizes = adjusted_split_sizes(&drag.start_sizes, drag.handle_index, delta_fraction);
        if let Some(tab) = self.active_tab_mut()
            && tab.root_pane.update_group_sizes(drag.group_id, &next_sizes)
        {
            cx.notify();
        }
    }

    fn finish_split_drag(&mut self, cx: &mut Context<Self>) {
        if self.split_drag.take().is_some() {
            cx.notify();
        }
    }

    fn switch_locale(&mut self, locale: Locale, window: &mut Window, cx: &mut Context<Self>) {
        self.i18n.set_locale(locale);
        self.sync_tab_titles(cx);

        let menus = crate::platform::app_menus(&self.i18n);
        let _ = cx.update_window(window.window_handle(), move |_root, _window, app| {
            app.set_menus(menus);
        });
        cx.notify();
    }

    fn sync_tab_titles(&mut self, cx: &App) {
        for tab in &mut self.tabs {
            if let Some(pane) = self.panes.get(&tab.active_pane_id) {
                let title = pane.read(cx).title().to_string();
                tab.title = if title.is_empty() {
                    self.i18n.t("terminal.local_terminal")
                } else {
                    title
                };
            }
        }
    }

    fn set_sidebar_section(&mut self, section: SidebarSection, cx: &mut Context<Self>) {
        self.active_sidebar_section = section;
        if self.sidebar_collapsed {
            self.sidebar_collapsed = false;
        }
        cx.notify();
    }

    fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar_collapsed = !self.sidebar_collapsed;
        self.sidebar_resizing = false;
        cx.notify();
    }

    fn sidebar_panel_width(&self) -> f32 {
        (self.sidebar_width - self.tokens.metrics.activity_bar_width).max(0.0)
    }

    fn set_sidebar_width(&mut self, width: f32, cx: &mut Context<Self>) {
        self.sidebar_width = width.clamp(
            self.tokens.metrics.sidebar_min_width,
            self.tokens.metrics.sidebar_max_width,
        );
        cx.notify();
    }

    fn start_sidebar_resize(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        self.sidebar_resizing = true;
        self.set_sidebar_width(f32::from(event.position.x), cx);
    }

    fn update_sidebar_resize(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if !self.sidebar_resizing {
            return;
        }
        if event.pressed_button != Some(MouseButton::Left) {
            return;
        }
        self.set_sidebar_width(f32::from(event.position.x), cx);
    }

    fn finish_sidebar_resize(&mut self, cx: &mut Context<Self>) {
        if self.sidebar_resizing {
            self.sidebar_resizing = false;
            cx.notify();
        }
    }

    fn render_title_bar(&self) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .h(px(self.tokens.metrics.titlebar_height))
            .flex()
            .items_center()
            .pl(px(self.tokens.metrics.titlebar_label_x()))
            .pr_2()
            .bg(rgb(theme.bg_active))
            .border_b_1()
            .border_color(rgb(theme.border))
            .text_size(px(self.tokens.metrics.titlebar_label_font_size))
            .text_color(rgb(theme.text_muted))
            .child(self.i18n.t("titlebar.open_recent_project"))
            .into_any_element()
    }

    fn render_activity_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let top_items = [
            (SidebarSection::Sessions, LucideIcon::Link2),
            (SidebarSection::Connections, LucideIcon::LayoutList),
            (SidebarSection::Terminal, LucideIcon::Terminal),
            (SidebarSection::Activity, LucideIcon::Activity),
            (SidebarSection::Network, LucideIcon::Network),
            (SidebarSection::Extensions, LucideIcon::Puzzle),
            (SidebarSection::Assistant, LucideIcon::Sparkles),
            (SidebarSection::Automation, LucideIcon::Bot),
        ];
        let bottom_items = [
            (SidebarSection::Workspace, LucideIcon::Square),
            (SidebarSection::Files, LucideIcon::FolderOpen),
            (SidebarSection::Monitor, LucideIcon::Monitor),
            (SidebarSection::Notifications, LucideIcon::Bell),
            (SidebarSection::Settings, LucideIcon::Settings),
        ];

        let mut bar = div()
            .w(px(self.tokens.metrics.activity_bar_width))
            .h_full()
            .flex()
            .flex_col()
            .items_center()
            .py_2()
            .bg(rgb(theme.bg))
            .border_r_1()
            .border_color(rgb(theme.border));

        bar = bar
            .child(
                div()
                    .size(px(self.tokens.metrics.activity_icon_size))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(self.tokens.radii.md))
                    .cursor_pointer()
                    .child(Self::render_lucide_icon(
                        if self.sidebar_collapsed {
                            LucideIcon::PanelLeft
                        } else {
                            LucideIcon::PanelLeftClose
                        },
                        self.tokens.metrics.activity_icon_glyph_size,
                        rgb(theme.text_heading),
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.toggle_sidebar(cx);
                        }),
                    ),
            )
            .child(
                div()
                    .w(px(self.tokens.metrics.divider_width))
                    .h(px(self.tokens.metrics.divider_height))
                    .my_1()
                    .bg(rgb(theme.divider)),
            );

        for (section, icon) in top_items {
            bar = bar.child(self.render_activity_icon(section, icon, cx));
        }

        bar.child(div().flex_1())
            .child(
                div()
                    .w(px(self.tokens.metrics.divider_width))
                    .h(px(self.tokens.metrics.divider_height))
                    .bg(rgb(theme.divider)),
            )
            .children(
                bottom_items
                    .into_iter()
                    .map(|(section, icon)| self.render_activity_icon(section, icon, cx)),
            )
            .into_any_element()
    }

    fn render_activity_icon(
        &self,
        section: SidebarSection,
        icon: LucideIcon,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active = self.active_sidebar_section == section;
        div()
            .id(("activity-icon", section as u64))
            .relative()
            .size(px(self.tokens.metrics.activity_icon_size))
            .mb(px(self.tokens.spacing.icon_gap))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(self.tokens.radii.md))
            .bg(if active {
                rgb(theme.bg_active)
            } else {
                rgb(theme.bg)
            })
            .border_1()
            .border_color(if active {
                rgb(theme.border)
            } else {
                rgb(theme.bg)
            })
            .cursor_pointer()
            .when(active, |icon_el| {
                icon_el.child(
                    div()
                        .absolute()
                        .left_0()
                        .top(px(self.tokens.metrics.activity_indicator_inset))
                        .bottom(px(self.tokens.metrics.activity_indicator_inset))
                        .w(px(self.tokens.metrics.activity_indicator_width))
                        .rounded(px(self.tokens.radii.active_indicator))
                        .bg(rgb(theme.accent)),
                )
            })
            .child(Self::render_lucide_icon(
                icon,
                self.tokens.metrics.activity_icon_glyph_size,
                if active {
                    rgb(theme.text_heading)
                } else {
                    rgb(theme.text)
                },
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.set_sidebar_section(section, cx);
                }),
            )
            .into_any_element()
    }

    fn render_lucide_icon(icon: LucideIcon, size: f32, color: Rgba) -> AnyElement {
        svg()
            .path(icon.path())
            .size(px(size))
            .text_color(color)
            .into_any_element()
    }

    fn render_sidebar_region(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .relative()
            .w(px(self.sidebar_panel_width()))
            .h_full()
            .child(self.render_sidebar(cx))
            .child(
                div()
                    .absolute()
                    .right_0()
                    .top_0()
                    .bottom_0()
                    .w(px(self.tokens.metrics.sidebar_resize_handle_width))
                    .cursor(CursorStyle::ResizeColumn)
                    .bg(if self.sidebar_resizing {
                        rgb(theme.accent)
                    } else {
                        rgba(theme.bg << 8)
                    })
                    .hover(|handle| handle.bg(rgba((theme.accent << 8) | 0x80)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event, _window, cx| {
                            this.start_sidebar_resize(event, cx);
                        }),
                    ),
            )
            .into_any_element()
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.bg_panel))
            .border_r_1()
            .border_color(rgb(theme.border))
            .child(self.render_sidebar_header(cx))
            .child(self.render_sidebar_content())
            .into_any_element()
    }

    fn render_sidebar_header(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .h(px(self.tokens.metrics.sidebar_header_height))
            .flex()
            .flex_row()
            .items_center()
            .px_2()
            .child(
                div()
                    .flex_1()
                    .truncate()
                    .text_size(px(self.tokens.metrics.sidebar_title_font_size))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t("sidebar.panels.sessions")),
            )
            .child(self.render_sidebar_action(LucideIcon::Folder, false, cx))
            .child(self.render_sidebar_action(LucideIcon::Network, false, cx))
            .child(self.render_sidebar_action(LucideIcon::Plus, true, cx))
            .into_any_element()
    }

    fn render_sidebar_action(
        &self,
        icon: LucideIcon,
        creates_terminal: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let mut button = div()
            .size(px(self.tokens.metrics.sidebar_action_size))
            .ml_1()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(self.tokens.radii.md))
            .cursor_pointer()
            .hover(move |button| button.bg(rgb(theme.bg_hover)))
            .child(Self::render_lucide_icon(
                icon,
                self.tokens.metrics.sidebar_action_icon_size,
                rgb(theme.text),
            ));

        if creates_terminal {
            button = button.on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, window, cx| {
                    let _ = this.create_local_terminal_tab(window, cx);
                }),
            );
        }

        button.into_any_element()
    }

    fn render_sidebar_content(&self) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .pt(px(self.tokens.metrics.empty_sidebar_top_padding))
            .text_color(rgb(theme.text_muted))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(div().mb_3().child(Self::render_lucide_icon(
                        LucideIcon::Server,
                        self.tokens.metrics.empty_sidebar_icon_size,
                        rgb(theme.bg_active),
                    )))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.empty_sidebar_title_font_size))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("sessions.tree.no_sessions")),
                    )
                    .child(
                        div()
                            .mt_2()
                            .text_size(px(self.tokens.metrics.empty_sidebar_subtitle_font_size))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("sessions.tree.click_to_add")),
                    ),
            )
            .into_any_element()
    }

    fn render_tab_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let mut bar = div()
            .h(px(self.tokens.metrics.tabbar_height))
            .flex()
            .flex_row()
            .items_center()
            .pl(px(self.tokens.metrics.tabbar_leading_offset))
            .pr_1()
            .border_b_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg_hover));

        for tab in &self.tabs {
            let tab_id = tab.id;
            let active = Some(tab_id) == self.active_tab_id;
            let title = tab.title.clone();
            let tab_label = match tab.kind {
                TabKind::LocalTerminal => format!(">_ {title}"),
            };
            bar = bar.child(
                div()
                    .id(("workspace-tab", tab_id.0))
                    .h_full()
                    .w(px(self.tokens.metrics.tab_width))
                    .px_2()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .border_r_1()
                    .border_color(rgb(theme.border))
                    .bg(if active {
                        rgb(theme.bg_active)
                    } else {
                        rgb(theme.bg_panel)
                    })
                    .text_color(if active {
                        rgb(theme.text)
                    } else {
                        rgb(theme.text_muted)
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, window, cx| {
                            this.set_active_tab(tab_id, window, cx);
                        }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .truncate()
                            .text_size(px(self.tokens.metrics.tab_font_size))
                            .child(tab_label),
                    )
                    .child(
                        div()
                            .px_1()
                            .cursor_pointer()
                            .text_size(px(self.tokens.metrics.tab_font_size))
                            .text_color(rgb(theme.text_muted))
                            .child("x")
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, window, cx| {
                                    this.set_active_tab(tab_id, window, cx);
                                    this.close_active_tab(window, cx);
                                }),
                            ),
                    ),
            );
        }

        bar.child(
            div()
                .id("workspace-new-tab")
                .h(px(self.tokens.metrics.new_tab_button_height))
                .w(px(self.tokens.metrics.new_tab_button_width))
                .ml_1()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(self.tokens.radii.sm))
                .bg(rgb(theme.bg_hover))
                .text_color(rgb(theme.text_muted))
                .text_size(px(self.tokens.metrics.empty_sidebar_title_font_size))
                .cursor_pointer()
                .child("+")
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, window, cx| {
                        let _ = this.create_local_terminal_tab(window, cx);
                    }),
                ),
        )
        .into_any_element()
    }

    fn render_search_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let query = if self.search.query.is_empty() {
            self.i18n.t("search.placeholder")
        } else {
            self.search.query.clone()
        };
        div()
            .h(px(self.tokens.metrics.searchbar_height))
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_2()
            .bg(rgb(theme.bg_panel))
            .border_b_1()
            .border_color(rgb(theme.border))
            .text_size(px(self.tokens.metrics.searchbar_font_size))
            .text_color(rgb(theme.text))
            .child(
                div()
                    .flex_1()
                    .h(px(self.tokens.metrics.search_input_height))
                    .px_2()
                    .flex()
                    .items_center()
                    .rounded(px(self.tokens.radii.sm))
                    .bg(rgb(theme.bg))
                    .text_color(if self.search.query.is_empty() {
                        rgb(theme.text_muted)
                    } else {
                        rgb(theme.text)
                    })
                    .child(query),
            )
            .child(
                div()
                    .px_2()
                    .cursor_pointer()
                    .child(self.i18n.t("search.previous"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.search_next(false, cx);
                        }),
                    ),
            )
            .child(
                div()
                    .px_2()
                    .cursor_pointer()
                    .child(self.i18n.t("search.next"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.search_next(true, cx);
                        }),
                    ),
            )
            .child(
                div()
                    .px_2()
                    .cursor_pointer()
                    .child(self.i18n.t("search.close"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, window, cx| {
                            this.close_search(window, cx);
                        }),
                    ),
            )
            .into_any_element()
    }

    fn render_empty_workspace(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgb(theme.bg))
            .child(
                div()
                    .px(px(self.tokens.metrics.empty_workspace_padding_x))
                    .py(px(self.tokens.metrics.empty_workspace_padding_y))
                    .rounded(px(self.tokens.radii.sm))
                    .bg(rgb(theme.bg_hover))
                    .text_color(rgb(theme.text))
                    .cursor_pointer()
                    .child(self.i18n.t("workspace.new_local_terminal"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, window, cx| {
                            let _ = this.create_local_terminal_tab(window, cx);
                        }),
                    ),
            )
            .into_any_element()
    }

    fn render_pane_tree(&self, node: &PaneNode, cx: &mut Context<Self>) -> AnyElement {
        match node {
            PaneNode::Leaf { pane_id, .. } => {
                let theme = self.tokens.ui;
                let active = Some(*pane_id) == self.active_pane_id();
                let Some(pane) = self.panes.get(pane_id).cloned() else {
                    return div().size_full().into_any_element();
                };
                div()
                    .id(("workspace-pane", pane_id.0))
                    .size_full()
                    .relative()
                    .min_w(px(self.tokens.metrics.min_pane_width))
                    .min_h(px(self.tokens.metrics.min_pane_height))
                    .overflow_hidden()
                    .border_1()
                    .border_color(if active {
                        rgb(theme.border)
                    } else {
                        rgb(theme.bg_panel)
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener({
                            let pane_id = *pane_id;
                            move |this, _event, window, cx| {
                                if let Some(tab) = this.active_tab_mut() {
                                    tab.active_pane_id = pane_id;
                                }
                                this.needs_active_pane_focus = true;
                                this.focus_active_pane(window, cx);
                                cx.notify();
                            }
                        }),
                    )
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .right_0()
                            .bottom_0()
                            .child(pane),
                    )
                    .into_any_element()
            }
            PaneNode::Group {
                id,
                direction,
                children,
                sizes,
            } => {
                let sizes = balanced_sizes(sizes, children.len());
                let mut group = div()
                    .id(("workspace-pane-group", id.0))
                    .size_full()
                    .flex()
                    .overflow_hidden();
                group = match direction {
                    SplitDirection::Horizontal => group.flex_row(),
                    SplitDirection::Vertical => group.flex_col(),
                };

                for (index, child) in children.iter().enumerate() {
                    let basis = relative(sizes.get(index).copied().unwrap_or(0.0) / 100.0);
                    group = group.child(
                        div()
                            .flex_none()
                            .flex_basis(basis)
                            .relative()
                            .min_w(px(self.tokens.metrics.min_pane_width))
                            .min_h(px(self.tokens.metrics.min_pane_height))
                            .overflow_hidden()
                            .child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .left_0()
                                    .right_0()
                                    .bottom_0()
                                    .child(self.render_pane_tree(child, cx)),
                            ),
                    );
                    if index + 1 < children.len() {
                        let group_id = *id;
                        let direction = *direction;
                        let start_sizes = sizes.clone();
                        let mut handle = div()
                            .flex_none()
                            .bg(rgb(self.tokens.ui.divider))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, event, _window, cx| {
                                    this.start_split_drag(
                                        group_id,
                                        index,
                                        direction,
                                        &start_sizes,
                                        event,
                                        cx,
                                    );
                                }),
                            );
                        handle = match direction {
                            SplitDirection::Horizontal => handle
                                .w(px(self.tokens.metrics.split_handle_size))
                                .h_full()
                                .cursor(CursorStyle::ResizeColumn),
                            SplitDirection::Vertical => handle
                                .h(px(self.tokens.metrics.split_handle_size))
                                .w_full()
                                .cursor(CursorStyle::ResizeRow),
                        };
                        group = group.child(handle);
                    }
                }

                group.into_any_element()
            }
        }
    }
}

impl Focusable for WorkspaceApp {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for WorkspaceApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_tab_titles(cx);
        let title = self
            .active_tab()
            .map(|tab| tab.title.clone())
            .unwrap_or_else(|| "OxideTerm".to_string());
        window.set_window_title(&SharedString::from(title));
        if self.needs_active_pane_focus
            && !self.search.visible
            && let Some(pane) = self.active_pane()
        {
            self.needs_active_pane_focus = false;
            window.on_next_frame(move |window, cx| {
                pane.read(cx).focus(window);
            });
        }

        let content = if let Some(tab) = self.active_tab() {
            self.render_pane_tree(&tab.root_pane, cx)
        } else {
            self.render_empty_workspace(cx)
        };

        div()
            .id("workspace-root")
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(self.tokens.ui.bg))
            .text_color(rgb(self.tokens.ui.text))
            .font_family(SharedString::from(self.tokens.metrics.font_family))
            .track_focus(&self.focus_handle)
            .key_context("Workspace")
            .on_key_down(cx.listener(|this, event, window, cx| {
                this.handle_workspace_key(event, window, cx);
            }))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                this.update_sidebar_resize(event, cx);
                this.update_split_drag(event, window, cx);
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                    this.finish_sidebar_resize(cx);
                    this.finish_split_drag(cx);
                }),
            )
            .on_action(cx.listener(|this, _: &NewTerminal, window, cx| {
                let _ = this.create_local_terminal_tab(window, cx);
            }))
            .on_action(cx.listener(|this, _: &CloseTab, window, cx| {
                this.close_active_tab(window, cx);
            }))
            .on_action(cx.listener(|this, _: &NextTab, window, cx| {
                this.next_tab(true, window, cx);
            }))
            .on_action(cx.listener(|this, _: &PrevTab, window, cx| {
                this.next_tab(false, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SplitHorizontal, window, cx| {
                this.split_active_pane(SplitDirection::Horizontal, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SplitVertical, window, cx| {
                this.split_active_pane(SplitDirection::Vertical, window, cx);
            }))
            .on_action(cx.listener(|this, _: &ClosePane, window, cx| {
                this.close_active_pane(window, cx);
            }))
            .on_action(cx.listener(|this, _: &Copy, _window, cx| this.copy(cx)))
            .on_action(cx.listener(|this, _: &Paste, _window, cx| this.paste(cx)))
            .on_action(cx.listener(|this, _: &Find, window, cx| {
                this.open_search(window, cx);
            }))
            .on_action(cx.listener(|this, _: &FindNext, _window, cx| {
                this.search_next(true, cx);
            }))
            .on_action(cx.listener(|this, _: &FindPrev, _window, cx| {
                this.search_next(false, cx);
            }))
            .on_action(cx.listener(|this, _: &CloseSearch, window, cx| {
                this.close_search(window, cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchLocaleEnglish, window, cx| {
                this.switch_locale(Locale::EnUs, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchLocaleChinese, window, cx| {
                this.switch_locale(Locale::ZhCn, window, cx);
            }))
            .on_action(cx.listener(|this, _: &GoToTab1, window, cx| {
                this.go_to_tab(0, window, cx);
            }))
            .on_action(cx.listener(|this, _: &GoToTab2, window, cx| {
                this.go_to_tab(1, window, cx);
            }))
            .on_action(cx.listener(|this, _: &GoToTab3, window, cx| {
                this.go_to_tab(2, window, cx);
            }))
            .on_action(cx.listener(|this, _: &GoToTab4, window, cx| {
                this.go_to_tab(3, window, cx);
            }))
            .on_action(cx.listener(|this, _: &GoToTab5, window, cx| {
                this.go_to_tab(4, window, cx);
            }))
            .on_action(cx.listener(|this, _: &GoToTab6, window, cx| {
                this.go_to_tab(5, window, cx);
            }))
            .on_action(cx.listener(|this, _: &GoToTab7, window, cx| {
                this.go_to_tab(6, window, cx);
            }))
            .on_action(cx.listener(|this, _: &GoToTab8, window, cx| {
                this.go_to_tab(7, window, cx);
            }))
            .on_action(cx.listener(|this, _: &GoToTab9, window, cx| {
                this.go_to_tab(8, window, cx);
            }))
            .child(self.render_title_bar())
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_row()
                    .overflow_hidden()
                    .child(self.render_activity_bar(cx))
                    .when(!self.sidebar_collapsed, |layout| {
                        layout.child(self.render_sidebar_region(cx))
                    })
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .min_w(px(self.tokens.metrics.min_main_width))
                            .overflow_hidden()
                            .child(self.render_tab_bar(cx))
                            .when(self.search.visible, |main| {
                                main.child(self.render_search_bar(cx))
                            })
                            .child(
                                div().flex_1().relative().overflow_hidden().child(
                                    div()
                                        .absolute()
                                        .top_0()
                                        .left_0()
                                        .right_0()
                                        .bottom_0()
                                        .child(content),
                                ),
                            ),
                    ),
            )
    }
}
