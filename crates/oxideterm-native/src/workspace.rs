use std::{collections::HashMap, time::Duration};

use anyhow::Result;
use gpui::{
    AnyElement, App, Context, CursorStyle, FocusHandle, Focusable, IntoElement, KeyDownEvent,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Render, Rgba,
    SharedString, Styled, Timer, Window, div, prelude::*, px, relative, rgb, rgba, svg,
};
use oxideterm_gpui_terminal::TerminalPane;
use oxideterm_i18n::{I18n, Locale};
use oxideterm_ssh::{
    AuthMethod, ConnectionConsumer, NodeId, NodeRouter, SshConfig, SshConnectionRegistry,
};
use oxideterm_terminal::SshSessionConfig;
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
    SwitchLocaleFrench, SwitchLocaleGerman, SwitchLocaleItalian, SwitchLocaleJapanese,
    SwitchLocaleKorean, SwitchLocalePortugueseBrazil, SwitchLocaleSpanish,
    SwitchLocaleTraditionalChinese, SwitchLocaleVietnamese,
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
enum SshAuthTab {
    Password,
    DefaultKey,
    SshKey,
    Certificate,
    Agent,
    TwoFactor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NewConnectionField {
    Name,
    Host,
    Port,
    Username,
    Password,
    Group,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConnectionButtonAction {
    Cancel,
    Test,
    Connect,
}

#[derive(Clone, Debug)]
struct NewConnectionForm {
    name: String,
    host: String,
    port: String,
    username: String,
    auth_tab: SshAuthTab,
    password: String,
    save_password: bool,
    group: String,
    agent_forwarding: bool,
    save_connection: bool,
    focused_field: NewConnectionField,
    error: Option<String>,
}

impl Default for NewConnectionForm {
    fn default() -> Self {
        Self {
            name: String::new(),
            host: String::new(),
            port: "22".to_string(),
            username: String::new(),
            auth_tab: SshAuthTab::Password,
            password: String::new(),
            save_password: false,
            group: String::new(),
            agent_forwarding: false,
            save_connection: true,
            focused_field: NewConnectionField::Name,
            error: None,
        }
    }
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
    new_connection_form: Option<NewConnectionForm>,
    new_connection_caret_visible: bool,
    ssh_registry: SshConnectionRegistry,
    node_router: NodeRouter,
    next_ssh_node_id: u64,
    i18n: I18n,
    tokens: ThemeTokens,
}

impl WorkspaceApp {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Result<Self> {
        let focus_handle = cx.focus_handle();
        let ssh_registry = SshConnectionRegistry::default();
        let node_router = NodeRouter::new(ssh_registry.clone());
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
            new_connection_form: None,
            new_connection_caret_visible: true,
            ssh_registry,
            node_router,
            next_ssh_node_id: 1,
            i18n: I18n::default(),
            tokens: default_tokens(),
        };
        cx.spawn(async move |weak, cx| {
            loop {
                Timer::after(Duration::from_millis(530)).await;
                if weak
                    .update(cx, |workspace, cx| {
                        if workspace.new_connection_form.is_some() {
                            workspace.new_connection_caret_visible =
                                !workspace.new_connection_caret_visible;
                            cx.notify();
                        } else if !workspace.new_connection_caret_visible {
                            workspace.new_connection_caret_visible = true;
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
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

    fn create_ssh_terminal_tab(
        &mut self,
        config: SshConfig,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let tab_id = self.alloc_tab_id();
        let pane_id = self.alloc_pane_id();
        let session_id = self.alloc_session_id();
        let node_id = NodeId::new(format!("ssh-{}", self.next_ssh_node_id));
        self.next_ssh_node_id += 1;

        self.node_router
            .upsert_node(node_id.clone(), config.clone());
        let _ = self.node_router.resolve_connection(
            &node_id,
            ConnectionConsumer::Terminal(session_id.0.to_string()),
        );
        let _pool_stats = self.ssh_registry.stats();

        let pane = cx.new(|cx| {
            TerminalPane::new_ssh(SshSessionConfig::from(config), window, cx)
                .expect("failed to initialize ssh terminal pane")
        });

        self.panes.insert(pane_id, pane.clone());
        self.tabs.push(Tab {
            id: tab_id,
            kind: TabKind::SshTerminal,
            title,
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

    fn open_new_connection_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.new_connection_form = Some(NewConnectionForm {
            group: self.i18n.t("ssh.form.ungrouped"),
            ..NewConnectionForm::default()
        });
        self.new_connection_caret_visible = true;
        self.needs_active_pane_focus = false;
        window.focus(&self.focus_handle);
        cx.notify();
    }

    fn close_new_connection_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.new_connection_form = None;
        self.focus_active_pane(window, cx);
        cx.notify();
    }

    fn submit_new_connection_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.new_connection_form.as_mut() else {
            return;
        };
        let host = form.host.trim().to_string();
        let username = form.username.trim().to_string();
        let port = form.port.trim().parse::<u16>().ok();
        if host.is_empty() || username.is_empty() || port.is_none() {
            form.error = Some(self.i18n.t("ssh.form.validation_required"));
            cx.notify();
            return;
        }

        let auth = match form.auth_tab {
            SshAuthTab::Password => AuthMethod::password(form.password.clone()),
            SshAuthTab::Agent => AuthMethod::Agent,
            SshAuthTab::DefaultKey | SshAuthTab::SshKey => AuthMethod::key("", None),
            SshAuthTab::Certificate => AuthMethod::certificate("", "", None),
            SshAuthTab::TwoFactor => AuthMethod::KeyboardInteractive,
        };
        let config = SshConfig {
            host: host.clone(),
            port: port.unwrap_or(22),
            username: username.clone(),
            auth,
            agent_forwarding: form.agent_forwarding,
            ..SshConfig::default()
        };
        let title = if form.name.trim().is_empty() {
            format!("{username}@{host}")
        } else {
            form.name.trim().to_string()
        };
        self.new_connection_form = None;
        let _ = self.create_ssh_terminal_tab(config, title, window, cx);
    }

    fn handle_new_connection_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(form) = self.new_connection_form.as_mut() else {
            return false;
        };
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;

        if modifiers.platform {
            if key == "v" {
                self.paste_into_new_connection_field(cx);
            }
            return true;
        }

        match key {
            "escape" => {
                self.close_new_connection_form(window, cx);
                true
            }
            "enter" => {
                self.submit_new_connection_form(window, cx);
                true
            }
            "tab" => {
                form.focused_field = next_connection_field(form.focused_field, !modifiers.shift);
                self.new_connection_caret_visible = true;
                cx.notify();
                true
            }
            "backspace" => {
                current_connection_field_mut(form).pop();
                form.error = None;
                self.new_connection_caret_visible = true;
                cx.notify();
                true
            }
            "space" => {
                current_connection_field_mut(form).push(' ');
                form.error = None;
                self.new_connection_caret_visible = true;
                cx.notify();
                true
            }
            key if key.chars().count() == 1 && !modifiers.control && !modifiers.alt => {
                current_connection_field_mut(form).push_str(key);
                form.error = None;
                self.new_connection_caret_visible = true;
                cx.notify();
                true
            }
            _ => true,
        }
    }

    fn paste_into_new_connection_field(&mut self, cx: &mut Context<Self>) {
        let Some(form) = self.new_connection_form.as_mut() else {
            return;
        };
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        let single_line = normalized.lines().collect::<Vec<_>>().join(" ");
        current_connection_field_mut(form).push_str(&single_line);
        form.error = None;
        self.new_connection_caret_visible = true;
        cx.notify();
    }

    fn handle_workspace_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.new_connection_form.is_some() {
            let _ = self.handle_new_connection_key(event, window, cx);
            return;
        }

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
        opens_connection_form: bool,
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

        if opens_connection_form {
            button = button.on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, window, cx| {
                    this.open_new_connection_form(window, cx);
                }),
            );
        }

        button.into_any_element()
    }

    fn render_sidebar_content(&self) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex_1()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .pt(px(self.tokens.metrics.empty_sidebar_top_padding))
            .px(px(self.tokens.metrics.empty_sidebar_padding_x))
            .text_color(rgb(theme.text_muted))
            .child(
                div()
                    .w_full()
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
                            .w_full()
                            .text_center()
                            .text_size(px(self.tokens.metrics.empty_sidebar_title_font_size))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("sessions.tree.no_sessions")),
                    )
                    .child(
                        div()
                            .mt_2()
                            .w_full()
                            .text_center()
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
                TabKind::SshTerminal => format!("⇄ {title}"),
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

    fn render_new_connection_modal(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(form) = self.new_connection_form.as_ref() else {
            return div().into_any_element();
        };
        let theme = self.tokens.ui;
        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba((theme.bg << 8) | 0xcc))
            .child(
                div()
                    .w(px(self.tokens.metrics.modal_width))
                    .rounded(px(self.tokens.radii.md))
                    .overflow_hidden()
                    .border_1()
                    .border_color(rgb(theme.border))
                    .bg(rgb(theme.bg_elevated))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .justify_center()
                            .px(px(self.tokens.metrics.modal_header_padding_x))
                            .py(px(self.tokens.metrics.modal_header_padding_y))
                            .bg(rgb(theme.bg_panel))
                            .border_b_1()
                            .border_color(rgb(theme.border))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.modal_title_font_size))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(theme.text_heading))
                                    .child(self.i18n.t("ssh.form.title")),
                            )
                            .child(
                                div()
                                    .mt_1()
                                    .text_size(px(self.tokens.metrics.modal_description_font_size))
                                    .text_color(rgb(theme.text_muted))
                                    .child(self.i18n.t("ssh.form.subtitle")),
                            ),
                    )
                    .child(
                        div()
                            .p(px(self.tokens.metrics.modal_body_padding))
                            .flex()
                            .flex_col()
                            .gap(px(self.tokens.metrics.modal_body_gap))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(self.tokens.metrics.modal_section_gap))
                                    .child(self.render_connection_field(
                                        self.i18n.t("ssh.form.name"),
                                        &form.name,
                                        self.i18n.t("ssh.form.name_placeholder"),
                                        NewConnectionField::Name,
                                        false,
                                        cx,
                                    ))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .gap(px(self.tokens.metrics.form_host_port_gap))
                                            .child(div().flex_1().child(
                                                self.render_connection_field(
                                                    self.i18n.t("ssh.form.host"),
                                                    &form.host,
                                                    self.i18n.t("ssh.form.host_placeholder"),
                                                    NewConnectionField::Host,
                                                    false,
                                                    cx,
                                                ),
                                            ))
                                            .child(
                                                div()
                                                    .w(px(self.tokens.metrics.form_port_width))
                                                    .child(self.render_connection_field(
                                                        self.i18n.t("ssh.form.port"),
                                                        &form.port,
                                                        "22".to_string(),
                                                        NewConnectionField::Port,
                                                        false,
                                                        cx,
                                                    )),
                                            ),
                                    )
                                    .child(self.render_connection_field(
                                        self.i18n.t("ssh.form.username"),
                                        &form.username,
                                        "root".to_string(),
                                        NewConnectionField::Username,
                                        false,
                                        cx,
                                    ))
                                    .child(self.render_auth_tabs(form.auth_tab, cx))
                                    .when(form.auth_tab == SshAuthTab::Password, |content| {
                                        content
                                            .child(self.render_connection_field(
                                                self.i18n.t("ssh.form.password"),
                                                &form.password,
                                                String::new(),
                                                NewConnectionField::Password,
                                                true,
                                                cx,
                                            ))
                                            .child(self.render_connection_checkbox(
                                                self.i18n.t("ssh.form.save_password"),
                                                form.save_password,
                                                |form| form.save_password = !form.save_password,
                                                cx,
                                            ))
                                    })
                                    .child(self.render_connection_field(
                                        self.i18n.t("ssh.form.group"),
                                        &form.group,
                                        self.i18n.t("ssh.form.ungrouped"),
                                        NewConnectionField::Group,
                                        false,
                                        cx,
                                    ))
                                    .child(self.render_connection_checkbox(
                                        self.i18n.t("ssh.form.agent_forwarding"),
                                        form.agent_forwarding,
                                        |form| form.agent_forwarding = !form.agent_forwarding,
                                        cx,
                                    ))
                                    .child(self.render_connection_checkbox(
                                        self.i18n.t("ssh.form.save_connection"),
                                        form.save_connection,
                                        |form| form.save_connection = !form.save_connection,
                                        cx,
                                    )),
                            )
                            .when_some(form.error.clone(), |content, error| {
                                content.child(
                                    div()
                                        .text_size(px(12.0))
                                        .text_color(rgb(theme.error))
                                        .child(error),
                                )
                            }),
                    )
                    .child(
                        div()
                            .h(px(self.tokens.metrics.modal_footer_height))
                            .px(px(self.tokens.metrics.modal_footer_padding_x))
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_end()
                            .gap_2()
                            .border_t_1()
                            .border_color(rgb(theme.border))
                            .bg(rgb(theme.bg_panel))
                            .child(self.render_connection_button(
                                self.i18n.t("ssh.form.cancel"),
                                false,
                                ConnectionButtonAction::Cancel,
                                cx,
                            ))
                            .child(self.render_connection_button(
                                self.i18n.t("ssh.form.test"),
                                false,
                                ConnectionButtonAction::Test,
                                cx,
                            ))
                            .child(self.render_connection_button(
                                self.i18n.t("ssh.form.connect"),
                                true,
                                ConnectionButtonAction::Connect,
                                cx,
                            )),
                    ),
            )
            .into_any_element()
    }

    fn render_connection_field(
        &self,
        label: String,
        value: &str,
        placeholder: String,
        field: NewConnectionField,
        secret: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let focused = self
            .new_connection_form
            .as_ref()
            .is_some_and(|form| form.focused_field == field);
        let display = if value.is_empty() {
            placeholder
        } else if secret {
            "•".repeat(value.chars().count())
        } else {
            value.to_string()
        };
        div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.modal_field_gap))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.form_label_font_size))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(theme.text))
                    .child(label),
            )
            .child(
                div()
                    .id(("connection-field", field as u32))
                    .h(px(self.tokens.metrics.form_input_height))
                    .px(px(self.tokens.metrics.form_input_padding_x))
                    .flex()
                    .items_center()
                    .rounded(px(self.tokens.radii.md))
                    .bg(rgba((theme.bg << 8) | 0x80))
                    .border_1()
                    .border_color(if focused {
                        rgb(theme.accent)
                    } else {
                        rgb(theme.border)
                    })
                    .text_size(px(self.tokens.metrics.form_text_font_size))
                    .text_color(if value.is_empty() {
                        rgb(theme.text_muted)
                    } else {
                        rgb(theme.text)
                    })
                    .cursor_pointer()
                    .overflow_hidden()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .when(
                                focused && value.is_empty() && self.new_connection_caret_visible,
                                |row| row.child(self.render_connection_caret()),
                            )
                            .child(
                                div()
                                    .text_color(if value.is_empty() {
                                        rgb(theme.text_muted)
                                    } else {
                                        rgb(theme.text)
                                    })
                                    .child(display),
                            )
                            .when(
                                focused && !value.is_empty() && self.new_connection_caret_visible,
                                |row| row.child(self.render_connection_caret()),
                            ),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, window, cx| {
                            if let Some(form) = this.new_connection_form.as_mut() {
                                form.focused_field = field;
                            }
                            this.new_connection_caret_visible = true;
                            window.focus(&this.focus_handle);
                            cx.notify();
                        }),
                    ),
            )
            .into_any_element()
    }

    fn render_connection_caret(&self) -> AnyElement {
        div()
            .w(px(self.tokens.metrics.form_caret_width))
            .h(px(self.tokens.metrics.form_caret_height))
            .bg(rgb(self.tokens.ui.accent))
            .into_any_element()
    }

    fn render_auth_tabs(&self, active_tab: SshAuthTab, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let tabs = [
            (SshAuthTab::Password, "ssh.auth.password"),
            (SshAuthTab::DefaultKey, "ssh.auth.default_key"),
            (SshAuthTab::SshKey, "ssh.auth.ssh_key"),
            (SshAuthTab::Certificate, "ssh.auth.certificate"),
            (SshAuthTab::Agent, "ssh.auth.agent"),
            (SshAuthTab::TwoFactor, "ssh.auth.two_factor"),
        ];
        let mut row = div()
            .h(px(self.tokens.metrics.auth_tab_height))
            .flex()
            .flex_row()
            .p(px(self.tokens.metrics.auth_tab_padding))
            .rounded(px(self.tokens.radii.xs))
            .overflow_hidden()
            .bg(rgb(theme.bg_panel));
        for (tab, key) in tabs {
            let selected = tab == active_tab;
            row = row.child(
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .rounded(px(self.tokens.radii.xs))
                    .bg(if selected {
                        rgb(theme.bg)
                    } else {
                        rgb(theme.bg_panel)
                    })
                    .text_size(px(self.tokens.metrics.form_text_font_size))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(if selected {
                        rgb(theme.text)
                    } else {
                        rgb(theme.text_muted)
                    })
                    .child(self.i18n.t(key))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            if let Some(form) = this.new_connection_form.as_mut() {
                                form.auth_tab = tab;
                            }
                            cx.notify();
                        }),
                    ),
            );
        }
        row.into_any_element()
    }

    fn render_connection_checkbox(
        &self,
        label: String,
        checked: bool,
        toggle: fn(&mut NewConnectionForm),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .cursor_pointer()
            .child(
                div()
                    .size(px(self.tokens.metrics.form_checkbox_size))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(self.tokens.radii.xs))
                    .border_1()
                    .border_color(if checked {
                        rgb(theme.accent)
                    } else {
                        rgb(theme.border)
                    })
                    .bg(if checked {
                        rgb(theme.accent)
                    } else {
                        rgb(theme.bg)
                    })
                    .text_size(px(self.tokens.metrics.form_checkbox_glyph_size))
                    .text_color(rgb(theme.accent_text))
                    .child(if checked { "✓" } else { "" }),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.form_text_font_size))
                    .text_color(rgb(theme.text))
                    .child(label),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if let Some(form) = this.new_connection_form.as_mut() {
                        toggle(form);
                    }
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn render_connection_button(
        &self,
        label: String,
        primary: bool,
        action: ConnectionButtonAction,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .h(px(self.tokens.metrics.form_button_height))
            .px(px(self.tokens.metrics.form_button_padding_x))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(if primary {
                rgb(theme.accent)
            } else {
                rgb(theme.bg_elevated)
            })
            .text_size(px(self.tokens.metrics.form_text_font_size))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(if primary {
                rgb(theme.accent_text)
            } else {
                rgb(theme.text)
            })
            .cursor_pointer()
            .child(label)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| match action {
                    ConnectionButtonAction::Cancel => {
                        this.close_new_connection_form(window, cx);
                    }
                    ConnectionButtonAction::Test => {
                        if let Some(form) = this.new_connection_form.as_mut() {
                            form.error = Some(this.i18n.t("ssh.form.test_pending"));
                        }
                        cx.notify();
                    }
                    ConnectionButtonAction::Connect => {
                        this.submit_new_connection_form(window, cx);
                    }
                }),
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
            && self.new_connection_form.is_none()
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
            .capture_key_down(cx.listener(|this, event, window, cx| {
                if this.new_connection_form.is_some() {
                    let _ = this.handle_new_connection_key(event, window, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                }
            }))
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
            .on_action(cx.listener(|this, _: &Copy, _window, cx| {
                if this.new_connection_form.is_none() {
                    this.copy(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Paste, _window, cx| {
                if this.new_connection_form.is_some() {
                    this.paste_into_new_connection_field(cx);
                } else {
                    this.paste(cx);
                }
            }))
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
                this.switch_locale(Locale::En, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchLocaleChinese, window, cx| {
                this.switch_locale(Locale::ZhCn, window, cx);
            }))
            .on_action(
                cx.listener(|this, _: &SwitchLocaleTraditionalChinese, window, cx| {
                    this.switch_locale(Locale::ZhTw, window, cx);
                }),
            )
            .on_action(cx.listener(|this, _: &SwitchLocaleGerman, window, cx| {
                this.switch_locale(Locale::De, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchLocaleSpanish, window, cx| {
                this.switch_locale(Locale::EsEs, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchLocaleFrench, window, cx| {
                this.switch_locale(Locale::FrFr, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchLocaleItalian, window, cx| {
                this.switch_locale(Locale::It, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchLocaleJapanese, window, cx| {
                this.switch_locale(Locale::Ja, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchLocaleKorean, window, cx| {
                this.switch_locale(Locale::Ko, window, cx);
            }))
            .on_action(
                cx.listener(|this, _: &SwitchLocalePortugueseBrazil, window, cx| {
                    this.switch_locale(Locale::PtBr, window, cx);
                }),
            )
            .on_action(cx.listener(|this, _: &SwitchLocaleVietnamese, window, cx| {
                this.switch_locale(Locale::Vi, window, cx);
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
            .when(self.new_connection_form.is_some(), |root| {
                root.child(self.render_new_connection_modal(cx))
            })
    }
}

fn next_connection_field(field: NewConnectionField, forward: bool) -> NewConnectionField {
    let fields = [
        NewConnectionField::Name,
        NewConnectionField::Host,
        NewConnectionField::Port,
        NewConnectionField::Username,
        NewConnectionField::Password,
        NewConnectionField::Group,
    ];
    let index = fields
        .iter()
        .position(|candidate| *candidate == field)
        .unwrap_or(0);
    let next = if forward {
        (index + 1) % fields.len()
    } else if index == 0 {
        fields.len() - 1
    } else {
        index - 1
    };
    fields[next]
}

fn current_connection_field_mut(form: &mut NewConnectionForm) -> &mut String {
    match form.focused_field {
        NewConnectionField::Name => &mut form.name,
        NewConnectionField::Host => &mut form.host,
        NewConnectionField::Port => &mut form.port,
        NewConnectionField::Username => &mut form.username,
        NewConnectionField::Password => &mut form.password,
        NewConnectionField::Group => &mut form.group,
    }
}
