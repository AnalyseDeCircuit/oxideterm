mod chrome;
mod sidebar;
use crate::{
    connections, demo::*, files::FileViews, palette::Palette, terminal::Session, tools::Tools,
    transport::*,
};
use futures::{StreamExt, channel::mpsc};
use gpui::{prelude::*, *};
use oxideterm_gpui_connections::{
    PortableCatalogData, PortableCatalogView,
    views::{ConnectionsEnvironment, PortableConnectionFormView},
};
use oxideterm_gpui_host_tools::ContextSidebarTool;
use oxideterm_gpui_settings_view::surface::{PortableSettingsChanged, PortableSettingsSurface};
use oxideterm_gpui_shell::{
    notifications::*,
    overlays::{transients::*, *},
    panes::*,
    shortcuts::{self, bindings::*},
    tabs::{
        TabLayout,
        geometry::{ExitingTabVisual, TabStripState, tab_exit_visual_index},
        strip::*,
    },
    *,
};
use oxideterm_gpui_ui::{
    disclosure_motion::DisclosureMotions,
    icons::LucideIcon,
    motion::{
        ExitPhase, MotionDuration, UserSegmentedControlMotionState, duration as motion_duration,
        exit_duration, exit_settle_duration, fade_in, settle_duration, slide_fade_in_y,
    },
};
use oxideterm_i18n::I18n;
use oxideterm_notification_center::*;
use oxideterm_theme::ThemeTokens;
use oxideterm_workspace::*;
use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
    time::Duration,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Terminal,
    Close,
    Split,
    Palette,
    Settings,
    Catalog,
    Notifications,
    NewConnection,
    Ble,
    Forwards,
    Quick,
    Welcome,
    Files,
    Sftp,
    Ide,
    Runtime,
    Plugins,
    CloudSync,
    Knowledge,
    Assistant,
    HostTool(ContextSidebarTool),
    Theme,
    Zen,
}
impl Action {
    pub fn key(self) -> &'static str {
        match self {
            Self::Assistant => "sidebar.panels.ai",
            Self::HostTool(tool) => tool.label_key(),
            Self::Zen => "sidebar.actions.enter_zen",
            Self::Theme => "settings_view.appearance.configure_schemes",
            Self::Terminal => "menu.new_terminal",
            Self::Close => "menu.close_tab",
            Self::Split => "menu.split_horizontal",
            Self::Palette => "command_palette.title",
            Self::Settings => "menu.settings",
            Self::Catalog => "sidebar.panels.connections_header",
            Self::NewConnection => "ssh.form.title",
            Self::Ble => "modals.new_connection.transport_ble",
            Self::Notifications => "event_log.audit.notifications",
            Self::Forwards => "sidebar.panels.forwards",
            Self::Quick => "terminal.quick_commands.title",
            Self::Welcome => "onboarding.step_welcome",
            Self::Files => "sidebar.panels.files",
            Self::Sftp => "sidebar.panels.sftp",
            Self::Ide => "connections.monitor.ide",
            Self::Runtime => "sidebar.panels.runtime",
            Self::Plugins => "plugin.manager_title",
            Self::CloudSync => "plugin.cloud_sync.panel_title",
            Self::Knowledge => "sidebar.panels.knowledge",
        }
    }
}
pub struct Workspace {
    pub focus: FocusHandle,
    pub settings: Entity<PortableSettingsSurface>,
    pub tokens: ThemeTokens,
    pub i18n: I18n,
    side: KeybindingSide,
    actions: Vec<ActionDefinition>,
    pub layout: TabLayout,
    rails: WorkspaceRails,
    strip: Entity<MeasuredTabStrip>,
    drag: Option<SplitDrag>,
    zen_exit: zen::ZenExitAffordance,
    pub palette: Palette,
    transients: TransientOverlays<String, ()>,
    deadline: Option<Task<()>>,
    notifications: NotificationState,
    center: Entity<NotificationCenter>,
    pub catalog: Entity<PortableCatalogView>,
    pub catalog_data: PortableCatalogData,
    pub next_profile: u64,
    pub form: Option<Entity<PortableConnectionFormView>>,
    pub form_close_generation: u64,
    pub disclosure_motions: DisclosureMotions,
    pub segmented_motion: UserSegmentedControlMotionState,
    pub popover_generation: usize,
    pub sessions: HashMap<TerminalSessionId, Session>,
    pub paired_ble: Option<crate::ble::PairedWebBle>,
    pub ble_profiles: HashMap<String, crate::ble::BleProfile>,
    pub ble_links: HashMap<TerminalSessionId, crate::ble::WebBleLink>,
    pub expanded_nodes: HashSet<String>,
    session_host_view: bool,
    session_sort_order: oxideterm_settings::SessionSortOrder,
    session_sort_menu_open: bool,
    session_search_open: bool,
    session_search_query: String,
    connections_sort_field: oxideterm_gpui_connections::session_manager::SessionSortField,
    connections_sort_direction: oxideterm_gpui_connections::session_manager::SortDirection,
    connections_sort_menu_open: bool,
    connections_search_open: bool,
    connections_search_query: String,
    selected_connection_id: Option<String>,
    connection_context_menu: Option<(String, Point<Pixels>)>,
    pub sidebar_search_focused: bool,
    pub pages: crate::pages::PagesState,
    pub knowledge: crate::knowledge::WebKnowledge,
    context_tools_menu: Option<ContextSidebarTool>,
    context_tools_menu_scroll: ScrollHandle,
    pub transport: Option<Box<dyn WebTransport>>,
    pub ready: bool,
    pub connecting: bool,
    gateway_host: Option<String>,
    pub demo: bool,
    _delivery: Task<()>,
    pub tools: Tools,
    pub files: Option<FileViews>,
    assistant: Option<Entity<crate::ai::Assistant>>,
}
impl Workspace {
    pub fn new(
        side: KeybindingSide,
        gateway: Option<String>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let demo = gateway.is_none();
        let actions = shortcuts::workspace_definitions();
        let mut preferences = oxideterm_settings::PersistedSettings::default();
        if let Some(language) = web_sys::window()
            .and_then(|w| w.navigator().language())
            .and_then(|tag| oxideterm_settings::Language::from_locale_tag(&tag))
        {
            preferences.general.language = language;
        }
        let sidebar = preferences.sidebar_ui.clone();
        let settings = cx.new(|cx| PortableSettingsSurface::new(preferences, actions.clone(), cx));
        settings.update(cx, |s, cx| {
            s.set_app_icon_picker(
                crate::icon::APP_ICON_VARIANTS,
                crate::icon::APP_ICON_PREVIEW_SIZE,
                crate::icon::app_icon_source,
                cx,
            );
        });
        crate::icon::sync_browser_favicon(settings.read(cx).settings(cx).appearance.app_icon);
        let (tokens, i18n) = settings.read(cx).environment(dark(w), cx);
        let focus = cx.focus_handle();
        let mut files = None;
        let (tx, mut rx) = mpsc::unbounded();
        // Show only the authority; paths and query parameters may contain credentials.
        let gateway_host = gateway
            .as_deref()
            .and_then(|s| url::Url::parse(s).ok())
            .and_then(|u| u.host_str().map(str::to_owned));
        let transport: Option<Box<dyn WebTransport>> = match gateway {
            Some(url) => WebSocketTransport::connect(&url, tx.clone())
                .map(|s| Box::new(s) as Box<dyn WebTransport>)
                .ok(),
            None => {
                let fs = DemoFiles::seeded();
                files = Some(FileViews::new(fs.clone(), tokens, &i18n, cx));
                Some(Box::new(DemoTransport::new(fs, tx.clone())))
            }
        };
        let wh = w.window_handle();
        let delivery = cx.spawn(async move |owner, cx| {
            while let Some(event) = rx.next().await {
                if wh
                    .update(cx, |_, w, cx| {
                        owner.update(cx, |h, cx| h.delivery(event, w, cx))
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        let owner = cx.weak_entity();
        let dispatch = owner.clone();
        let strip = cx.new(|_| {
            MeasuredTabStrip::new(
                move |s, w, cx| owner.update(cx, |h, cx| h.strip_frame(s, w, cx)).ok(),
                move |req, w, cx| {
                    let _ = dispatch.update(cx, |h, cx| h.tab_request(req, w, cx));
                },
            )
        });
        let catalog_data = connections::catalog(demo);
        let catalog = cx.new(|cx| {
            PortableCatalogView::new(
                catalog_data.clone(),
                ConnectionsEnvironment {
                    tokens,
                    i18n: i18n.clone(),
                },
                cx,
            )
        });
        cx.subscribe_in(&catalog, w, |h, _, event, w, cx| {
            h.catalog_event(event, w, cx)
        })
        .detach();
        cx.subscribe_in(&settings, w, |h, _, _: &PortableSettingsChanged, w, cx| {
            h.preferences(w, cx)
        })
        .detach();
        let notifications = NotificationState::default();
        let frame = notification_frame(tokens, &i18n, &notifications, cx.weak_entity());
        let center = cx.new(|_| NotificationCenter::new(frame, None));
        let palette = Palette::new(cx);
        let tools = Tools::new(demo, tokens, &i18n, &focus, cx);
        let knowledge = crate::knowledge::WebKnowledge::new(&focus, cx);
        let open_ai_settings_owner = cx.weak_entity();
        let insert_cmd_owner = cx.weak_entity();
        let assistant = demo.then(|| {
            let settings_handle = settings.clone();
            cx.new(|cx| {
                crate::ai::Assistant::new(
                    tokens,
                    i18n.clone(),
                    settings_handle,
                    Rc::new(move |w, cx| {
                        let _ = open_ai_settings_owner.update(cx, |h, cx| {
                            h.settings.update(cx, |s, cx| {
                                s.open_section(oxideterm_gpui_settings_view::SettingsTab::Ai, cx);
                            });
                            h.action(Action::Settings, w, cx);
                        });
                    }),
                    Rc::new(move |cmd, _, cx| {
                        let _ = insert_cmd_owner.update(cx, |h, cx| {
                            if let Some(sid) = h.active_session() {
                                h.input(sid, cmd.as_bytes(), cx);
                                cx.notify();
                            }
                        });
                    }),
                    cx,
                )
            })
        });
        Self {
            assistant,
            focus,
            settings,
            tokens,
            i18n,
            side,
            actions,
            layout: Default::default(),
            rails: WorkspaceRails::new(
                RailState::new(
                    RailSide::Left,
                    sidebar.width as f32,
                    sidebar.collapsed,
                    !sidebar.collapsed,
                    &tokens,
                ),
                RailState::new(
                    RailSide::Right,
                    sidebar.ai_sidebar_width as f32,
                    sidebar.ai_sidebar_collapsed,
                    !sidebar.ai_sidebar_collapsed,
                    &tokens,
                ),
                SidebarSection::Sessions,
                ContextSidebarPanel::HostTools,
            ),
            strip,
            drag: None,
            zen_exit: Default::default(),
            palette,
            transients: TransientOverlays::new(exit_settle_duration(
                &tokens,
                MotionDuration::Control,
            )),
            deadline: None,
            notifications,
            center,
            catalog,
            catalog_data,
            next_profile: 0,
            form: None,
            form_close_generation: 0,
            disclosure_motions: DisclosureMotions::default(),
            segmented_motion: UserSegmentedControlMotionState::default(),
            popover_generation: 0,
            sessions: Default::default(),
            paired_ble: None,
            ble_profiles: Default::default(),
            ble_links: Default::default(),
            expanded_nodes: HashSet::from(["demo".to_string(), "local".to_string()]),
            session_host_view: false,
            session_sort_order: oxideterm_settings::SessionSortOrder::Default,
            session_sort_menu_open: false,
            session_search_open: false,
            session_search_query: String::new(),
            connections_sort_field:
                oxideterm_gpui_connections::session_manager::SessionSortField::Name,
            connections_sort_direction:
                oxideterm_gpui_connections::session_manager::SortDirection::Asc,
            connections_sort_menu_open: false,
            connections_search_open: false,
            connections_search_query: String::new(),
            selected_connection_id: None,
            connection_context_menu: None,
            sidebar_search_focused: false,
            pages: Default::default(),
            knowledge,
            context_tools_menu: None,
            context_tools_menu_scroll: ScrollHandle::new(),
            connecting: !demo && transport.is_some(),
            gateway_host,
            transport,
            ready: demo,
            demo,
            _delivery: delivery,
            tools,
            files,
        }
    }
    pub fn start(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        w.focus(&self.focus, cx);
        if self.demo {
            // Start with the demo SSH host active so the sidebar session tree
            // immediately presents desktop's full connected host actions.
            self.new_terminal("demo".into(), None, w, cx)
        }
    }
    fn preferences(&mut self, w: &Window, cx: &mut Context<Self>) {
        (self.tokens, self.i18n) = self.settings.read(cx).environment(dark(w), cx);
        crate::icon::sync_browser_favicon(self.settings.read(cx).settings(cx).appearance.app_icon);
        let env = self.connection_env();
        self.catalog
            .update(cx, |v, cx| v.set_environment(env.clone(), cx));
        if let Some(f) = &self.form {
            f.update(cx, |v, cx| v.set_environment(env, cx));
        }
        self.layout.sync_tab_titles(|key| self.i18n.t(key));
        self.refresh_terminals(w, cx);
        App::notify(cx, self.tools.onboarding.entity_id());
        App::notify(cx, self.tools.forwards.state.entity_id());
        if let Some(v) = &self.tools.quick.views {
            App::notify(cx, v.manager.entity_id());
            App::notify(cx, v.dock.entity_id());
        }
        if let Some(f) = &self.files {
            f.refresh(self.tokens, &self.i18n, cx);
        }
        if let Some(a) = &self.assistant {
            a.update(cx, |a, cx| {
                a.environment(self.tokens, self.i18n.clone(), cx)
            });
        }
        if let Some(m) = &self.tools.monitor {
            m.update(cx, |m, cx| {
                m.tokens = self.tokens;
                m.i18n = self.i18n.clone();
                cx.notify();
            });
        }
        App::notify(cx, self.knowledge.views.view.entity_id());
        self.refresh_center(cx);
        self.changed(cx);
    }
    pub fn changed(&self, cx: &mut Context<Self>) {
        App::notify(cx, self.strip.entity_id());
        cx.notify();
    }
    pub fn notice(&mut self, key: &str, cx: &mut Context<Self>) {
        self.notifications_notice(self.i18n.t(key), cx);
    }
    pub fn notifications_notice(&mut self, title: String, cx: &mut Context<Self>) {
        self.transients
            .push_notice(title.clone(), Duration::from_secs(4), None, Instant::now());
        self.arm_deadline(cx);
        self.notifications.push(
            NotificationKind::Connection,
            NotificationSeverity::Info,
            title,
            None,
            NotificationScope::Global,
            None,
        );
        self.refresh_center(cx);
        cx.notify();
    }
    fn refresh_center(&self, cx: &mut Context<Self>) {
        let frame = notification_frame(
            self.tokens,
            &self.i18n,
            &self.notifications,
            cx.weak_entity(),
        );
        self.center.update(cx, |v, cx| v.update_frame(frame, cx));
    }
    pub fn open_page(&mut self, kind: TabKind, key: &'static str, cx: &mut Context<Self>) {
        let existing = self
            .layout
            .tabs()
            .iter()
            .find(|t| t.kind == kind)
            .map(|t| t.id);
        if let Some(id) = existing {
            self.layout.select_main_tab(Some(id));
        } else {
            let id = self.layout.alloc_tab_id();
            self.layout.insert_and_select_main_tab(Tab {
                id,
                kind,
                title: self.i18n.t(key),
                title_source: TabTitleSource::I18nKey(key),
                root_pane: None,
                active_pane_id: None,
            });
        }
        self.changed(cx);
    }
    pub(crate) fn begin_disclosure(&mut self, key: String, expanded: bool, cx: &mut Context<Self>) {
        if !self.tokens.motion.enabled {
            self.disclosure_motions.remove(&key);
            return;
        }
        let generation = self.disclosure_motions.begin(key.clone(), expanded);
        let delay = motion_duration(&self.tokens, MotionDuration::Micro);
        at_deadline(Instant::now() + delay, cx, move |h, _, cx| {
            if h.disclosure_motions.finish(&key, generation) {
                cx.notify();
            }
        })
        .detach();
    }

    pub(crate) fn begin_segmented_transition(
        &mut self,
        control_id: &'static str,
        previous_index: usize,
        target_index: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(motion) = oxideterm_gpui_ui::segmented_control_motion(&self.tokens) else {
            self.segmented_motion.clear(control_id);
            return;
        };
        let generation = self.segmented_motion.begin_with_previous_index(
            control_id,
            Some(previous_index),
            target_index,
            None,
        );
        at_deadline(Instant::now() + motion.duration, cx, move |h, _, cx| {
            if h.segmented_motion.finish(control_id, generation) {
                cx.notify();
            }
        })
        .detach();
    }

    pub fn close(&mut self, id: TabId, cx: &mut Context<Self>) {
        let Some(index) = self.layout.tab_index_by_id(id) else {
            return;
        };
        let exit_delay = exit_duration(&self.tokens, MotionDuration::Control);
        let exiting_visual = if self.tokens.motion.enabled && !exit_delay.is_zero() {
            let tab = &self.layout.tabs()[index];
            self.layout
                .main_tabs()
                .position(|(_, candidate)| candidate.id == tab.id)
                .map(|live_visual_index| {
                    let mut occupied = self
                        .strip
                        .read(cx)
                        .state
                        .exiting_tabs
                        .iter()
                        .map(|e| e.visual_index)
                        .collect::<Vec<_>>();
                    occupied.sort_unstable();
                    let visual_index = tab_exit_visual_index(live_visual_index, &occupied);
                    ExitingTabVisual {
                        tab_id: tab.id,
                        kind: tab.kind.clone(),
                        title: tab.title.clone(),
                        width: self.tokens.metrics.tab_min_width.max(140.0),
                        visual_index,
                        was_active: self.layout.active_tab_id() == Some(tab.id),
                        started_at: cx.background_executor().now(),
                        duration: exit_delay,
                    }
                })
        } else {
            None
        };
        let mut sessions = Vec::new();
        if let Some(p) = &self.layout.tabs()[index].root_pane {
            p.collect_session_ids(&mut sessions);
        }
        self.layout.remove_tab_at(index);
        if let Some(visual) = exiting_visual {
            self.strip
                .update(cx, |strip, cx| strip.begin_exit(visual, cx));
        }
        self.transients
            .clear_tooltip(&format!("tab-{}", id.0), Instant::now());
        for s in sessions {
            self.release_session(s, cx);
        }
        self.changed(cx);
    }
    pub fn disconnect_node(&mut self, node_id: &str, cx: &mut Context<Self>) {
        let target_tabs: Vec<TabId> = self
            .layout
            .tabs()
            .iter()
            .filter_map(|tab| {
                let root = tab.root_pane.as_ref()?;
                let mut ids = Vec::new();
                root.collect_session_ids(&mut ids);
                ids.iter()
                    .any(|sid| self.sessions.get(sid).is_some_and(|s| s.node == node_id))
                    .then_some(tab.id)
            })
            .collect();
        for tab_id in target_tabs {
            self.close(tab_id, cx);
        }
        self.expanded_nodes.remove(node_id);
        self.changed(cx);
    }
    pub fn action(&mut self, a: Action, w: &mut Window, cx: &mut Context<Self>) {
        use self::Action::*;
        self.sidebar_search_focused = false;
        match a {
            Terminal => self.new_terminal("local".into(), None, w, cx),
            Close => {
                if let Some(id) = self.layout.active_tab_id() {
                    self.close(id, cx)
                }
            }
            Split => {
                if let Some(node) = self
                    .active_session()
                    .and_then(|s| self.sessions.get(&s))
                    .map(|s| s.node.clone())
                {
                    self.new_terminal(node, Some(SplitDirection::Horizontal), w, cx)
                }
            }
            Palette => {
                self.palette.open(&self.focus, w, cx);
                self.changed(cx);
            }
            Theme => self.toggle_theme(w, cx),
            Zen => self.toggle_zen(cx),
            Settings => self.open_page(TabKind::Settings, a.key(), cx),
            Notifications => self.open_page(TabKind::NotificationCenter, a.key(), cx),
            Welcome => self.open_page(TabKind::Onboarding, a.key(), cx),
            Ble => self.form_for_transport(
                oxideterm_gpui_connections::form_state::NewConnectionTransport::Ble,
                w,
                cx,
            ),
            _ if !self.demo => self.notice("remote_desktop.capability_unsupported", cx),
            Catalog => self.open_page(TabKind::SessionManager, a.key(), cx),
            HostTool(tool) => {
                if let Some(m) = &self.tools.monitor {
                    let close = !self.rails.right.collapsed()
                        && self.rails.context_panel == ContextSidebarPanel::HostTools
                        && m.read(cx).active_tool(cx) == tool;
                    m.update(cx, |m, cx| m.select(tool, cx));
                    self.rails.context_panel = ContextSidebarPanel::HostTools;
                    self.collapse(RailSide::Right, close, cx);
                    self.changed(cx);
                }
            }
            Assistant => {
                self.context_tools_menu = None;
                let close = !self.rails.right.collapsed()
                    && self.rails.context_panel == ContextSidebarPanel::Assistant;
                self.rails.context_panel = ContextSidebarPanel::Assistant;
                self.collapse(RailSide::Right, close, cx);
                self.changed(cx);
            }
            NewConnection => self.form(None, None, w, cx),
            Files => self.open_page(TabKind::FileManager, a.key(), cx),
            Sftp => self.open_page(TabKind::Sftp, a.key(), cx),
            Ide => self.open_page(TabKind::Ide, a.key(), cx),
            Forwards => self.open_page(TabKind::Forwards, a.key(), cx),
            Runtime => self.open_page(TabKind::Runtime, a.key(), cx),
            Plugins => self.open_page(TabKind::PluginManager, a.key(), cx),
            CloudSync => self.open_page(TabKind::CloudSync, a.key(), cx),
            Knowledge => {
                self.open_page(TabKind::Knowledge, a.key(), cx);
                if let Some(tab_id) = self.layout.active_tab_id() {
                    self.knowledge.views.register_tab(tab_id, cx);
                }
                self.knowledge.views.refresh(cx);
            }
            Quick => {
                if self.active_session().is_some() {
                    self.tools.quick.toggle_open();
                } else {
                    self.tools.quick.open_manager();
                }
                w.focus(&self.focus, cx);
                self.changed(cx);
            }
        }
    }
    pub fn frame(&mut self, w: &mut Window, cx: &mut Context<Self>) -> ShellContent {
        let t = self.tokens;
        let bg = rgb(t.ui.bg);
        let border = self.chrome_divider();
        let settings = self.settings.read(cx).settings(cx);
        let zen = settings.sidebar_ui.zen_mode;
        let tabbar_visible = !zen || settings.appearance.zen_mode_keep_tabbar;
        let env = ShellEnvironment {
            tokens: t,
            i18n: self.i18n.clone(),
            zen_mode: zen,
            chrome_locked: self.form.is_some() || self.palette.model.read(cx).is_open(),
            chrome_background: rgb(t.ui.shell_bg()),
            divider: border,
            left_divider: rgba((t.ui.border << 8) | 0x40),
        };
        let center = if let Some(tab) = self.layout.active_tab().cloned() {
            if let Some(root) = tab.root_pane {
                PaneTree {
                    tab: tab.id,
                    active: tab.active_pane_id,
                    zoomed: self.layout.zoomed_pane(tab.id),
                    drag: self.drag.clone(),
                    supports_detach: false,
                    drop_regions: None,
                    tokens: t,
                    i18n: &self.i18n.clone(),
                    header_background: rgb(t.ui.shell_bg()),
                    divider: border,
                }
                .render(&root, self, w, cx)
            } else {
                fade_in(
                    &t,
                    ("web-page-enter", tab.id.0 as usize),
                    div().size_full().child(self.page(&tab, cx)),
                    MotionDuration::Micro,
                )
            }
        } else {
            slide_fade_in_y(
                &t,
                "web-empty-workspace-enter",
                div().size_full().child(self.empty_workspace(w, cx)),
                8.0,
                MotionDuration::Control,
            )
        };
        let top = t.metrics.titlebar_height;
        let left = self.rails.left.mounted().then(|| self.left_drawer(cx));
        let left_panel = self
            .rails
            .left
            .present(left, !self.rails.left.collapsed(), &env, top, cx);
        let right = self
            .rails
            .right
            .mounted()
            .then(|| self.right_drawer(cx))
            .flatten();
        let right_panel =
            self.rails
                .right
                .present(right, !self.rails.right.collapsed(), &env, top, cx);
        let root = div()
            .id("web-workspace")
            .track_focus(&self.focus)
            .font_family("IBM Plex Sans")
            .text_color(rgb(t.ui.text))
            .bg(bg)
            .capture_key_down(cx.listener(Self::key))
            .on_mouse_move(cx.listener(|h, e: &MouseMoveEvent, w, cx| {
                if let Some(d) = &h.drag {
                    h.pane_request(d.resize_at(e.position), w, cx);
                }
                if h.rails.resizing_side().is_some() {
                    h.request(ShellRequest::DragResize(e.position.x), w, cx);
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|h, _, w, cx| {
                    h.drag = None;
                    h.request(ShellRequest::EndResize, w, cx)
                }),
            );
        let mut overlays = OverlayStack::default();
        if let Some(menu) = self.render_context_tools_menu(w, cx) {
            overlays.at(60);
            overlays.extend([menu]);
        }
        if let Some(menu) = self.render_session_sort_menu(cx) {
            overlays.at(62);
            overlays.extend([menu]);
        }
        if let Some(menu) = self.render_connections_sort_menu(cx) {
            overlays.at(62);
            overlays.extend([menu]);
        }
        if let Some(menu) = self.render_connection_context_menu(cx) {
            overlays.at(64);
            overlays.extend([menu]);
        }
        if let Some(form) = &self.form {
            overlays.at(FORM);
            overlays.extend([form.clone().into_any_element()]);
        }
        if self.tools.quick.manager_open() || self.tools.quick.is_open() {
            overlays.at(QUICK);
            overlays.extend(self.tools.quick.views.as_ref().map(|v| {
                if self.tools.quick.manager_open() {
                    v.manager.clone()
                } else {
                    v.dock.clone()
                }
                .into_any_element()
            }));
        }
        if self.palette.model.read(cx).is_open() {
            overlays.at(PALETTE);
            overlays.extend([self.palette.view.clone().into_any_element()]);
        }
        if zen && !tabbar_visible {
            overlays.at(10);
            overlays.extend([self.zen_exit.hot_zone(px(0.), cx)]);
            if self.zen_exit.visible() {
                overlays.extend([self.zen_exit.pill(
                    px(0.),
                    &env,
                    self.shortcut("app.zenMode", cx).unwrap_or_default(),
                    cx,
                )]);
            }
        }
        overlays.at(200);
        let owner = cx.weak_entity();
        overlays.extend([self
            .transients
            .render_toasts(
                &t,
                |title| ToastContent {
                    title: title.clone(),
                    description: None,
                    status_text: None,
                    progress: None,
                    variant: oxideterm_gpui_ui::toast::ToastVariant::Default,
                },
                |_| String::new(),
                Rc::new(move |req, _, cx| {
                    if let ToastRequest::Dismiss(id) = req {
                        let _ = owner.update(cx, |h, cx| {
                            h.transients.begin_standard_exit(id, Instant::now());
                            h.arm_deadline(cx);
                            cx.notify();
                        });
                    }
                }),
            )
            .into_any_element()]);
        overlays.extend(self.transients.render_tooltip(&t));
        ShellContent::Frame(ShellFrameSlots {
            root,
            background: None,
            title_row: (!zen).then(|| self.titlebar(cx)),
            left_rail: Some(self.left_activity_rail(&env, cx)),
            right_rail: self.demo.then(|| self.right_activity_rail(w, &env, cx)),
            left_panel,
            right_panel,
            center,
            tab_bar: tabbar_visible.then(|| self.strip.clone().into_any_element()),
            overlays,
            environment: env,
        })
    }
    fn page(&self, tab: &Tab, cx: &Context<Self>) -> AnyElement {
        match tab.kind {
            TabKind::Settings => self.settings.clone().into_any_element(),
            TabKind::SessionManager => self.catalog.clone().into_any_element(),
            TabKind::NotificationCenter => self.center.clone().into_any_element(),
            TabKind::Onboarding => self.tools.onboarding.clone().into_any_element(),
            TabKind::Forwards => div()
                .relative()
                .size_full()
                .child(self.tools.forwards.body())
                .child(self.tools.forwards.dialogs())
                .into_any_element(),
            TabKind::Runtime => self.render_runtime_page(cx),
            TabKind::PluginManager => self.render_plugin_manager_page(cx),
            TabKind::CloudSync => self.render_cloud_sync_page(cx),
            TabKind::Knowledge => self.knowledge.views.view.clone().into_any_element(),
            _ => self
                .files
                .as_ref()
                .map(|f| f.page(&tab.kind, cx))
                .unwrap_or_else(|| div().into_any_element()),
        }
    }
    fn key(&mut self, e: &KeyDownEvent, w: &mut Window, cx: &mut Context<Self>) {
        if (self.context_tools_menu.is_some()
            || self.session_sort_menu_open
            || self.connections_sort_menu_open
            || self.connection_context_menu.is_some())
            && e.keystroke.key == "escape"
        {
            self.context_tools_menu = None;
            self.session_sort_menu_open = false;
            self.connections_sort_menu_open = false;
            self.connection_context_menu = None;
            w.prevent_default();
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if self.knowledge_input_key(e, w, cx) {
            w.prevent_default();
            cx.stop_propagation();
            return;
        }
        if self.plugin_search_key(e, cx) {
            w.prevent_default();
            cx.stop_propagation();
            return;
        }
        if self.sidebar_search_focused
            && !e.keystroke.modifiers.platform
            && !e.keystroke.modifiers.control
        {
            let is_conn =
                self.rails.section == SidebarSection::Connections && self.connections_search_open;
            let is_sess =
                self.rails.section == SidebarSection::Sessions && self.session_search_open;
            if is_conn || is_sess {
                match e.keystroke.key.as_str() {
                    "escape" => {
                        if is_conn {
                            self.connections_search_query.clear();
                            self.connections_search_open = false;
                            self.begin_disclosure("connections-search".into(), false, cx);
                        } else {
                            self.session_search_query.clear();
                            self.session_search_open = false;
                            self.begin_disclosure("session-search".into(), false, cx);
                        }
                        self.sidebar_search_focused = false;
                        w.prevent_default();
                        cx.stop_propagation();
                        cx.notify();
                        return;
                    }
                    "backspace" => {
                        if is_conn {
                            self.connections_search_query.pop();
                        } else {
                            self.session_search_query.pop();
                        }
                        w.prevent_default();
                        cx.stop_propagation();
                        cx.notify();
                        return;
                    }
                    _ => {
                        if let Some(text) = &e.keystroke.key_char
                            && !text.chars().any(char::is_control)
                        {
                            if is_conn {
                                self.connections_search_query.push_str(text);
                            } else {
                                self.session_search_query.push_str(text);
                            }
                            w.prevent_default();
                            cx.stop_propagation();
                            cx.notify();
                            return;
                        }
                    }
                }
            } else {
                self.sidebar_search_focused = false;
            }
        }
        let layers = [
            (FORM, self.form.is_some()),
            (
                QUICK,
                self.tools.quick.manager_open() || self.tools.quick.is_open(),
            ),
            (PALETTE, self.palette.model.read(cx).is_open()),
        ];
        if let Some(layer) = top_blocking(layers.into_iter().filter(|(_, on)| *on).map(
            |(priority, _)| ModalLayer {
                owner: priority,
                priority,
                phase: ExitPhase::Visible,
                editor: priority != PALETTE,
            },
        )) {
            if layer.owner == PALETTE {
                self.palette_key(&e.keystroke, w, cx);
            } else if layer.owner == QUICK {
                if !self.quick_key(e, cx) {
                    return;
                }
            } else {
                return;
            }
        } else if self.forwards_key(e, cx)
            || self
                .layout
                .active_tab()
                .is_some_and(|t| t.kind == TabKind::FileManager)
                && self.files.as_ref().is_some_and(|f| f.manager_key(e, cx))
        {
        } else {
            let Some((def, _)) = shortcuts::matched_action_for_keystroke(
                &self.actions,
                self.side,
                &e.keystroke,
                &self.settings.read(cx).settings(cx).keybindings.overrides,
            ) else {
                return;
            };
            let a = match def.id.as_ref() {
                "app.newTerminal" => Action::Terminal,
                "app.closeTab" => Action::Close,
                "app.zenMode" => Action::Zen,
                "app.settings" => Action::Settings,
                "app.commandPalette" => Action::Palette,
                "app.newConnection" => Action::NewConnection,
                "split.horizontal" => Action::Split,
                "app.toggleSidebar" => {
                    self.request(ShellRequest::Toggle(RailSide::Left), w, cx);
                    cx.stop_propagation();
                    return;
                }
                _ => return,
            };
            self.action(a, w, cx);
        }
        w.prevent_default();
        cx.stop_propagation();
    }
    fn strip_frame(&self, s: &TabStripState, w: &Window, cx: &Context<Self>) -> TabStripFrame {
        let t = self.tokens;
        let tabs = self.layout.main_tabs().map(|(_, t)| t).collect::<Vec<_>>();
        let layout = s.layout(
            tabs.iter().map(|tab| {
                (
                    tab.id,
                    s.natural_width(
                        &tab.title,
                        t.metrics.tab_close_button_size,
                        "IBM Plex Sans".into(),
                        &t,
                        w,
                    ),
                )
            }),
            None,
            &t,
            Instant::now(),
        );
        let bells = self.layout.bell_highlighted_tab_ids();
        let unread = self.layout.unread_terminal_output_tab_ids();
        let (quick_actions, quick_actions_width) = self.strip_actions(cx);
        TabStripFrame {
            tokens: t,
            layout,
            visuals: tabs
                .iter()
                .map(|tab| {
                    let is_ble_tab = tab.root_pane.as_ref().is_some_and(|root| {
                        let mut ids = Vec::new();
                        root.collect_session_ids(&mut ids);
                        ids.iter().any(|sid| {
                            self.sessions
                                .get(sid)
                                .is_some_and(|s| crate::ble::is_ble_node(&s.node))
                        })
                    });
                    let icon = if is_ble_tab {
                        LucideIcon::Bluetooth
                    } else {
                        oxideterm_gpui_shell::chrome::tab_icon(&tab.kind)
                    };
                    TabVisual {
                        title: tab.title.clone(),
                        icon: svg()
                            .path(icon.path())
                            .text_color(TabVisual::color(
                                &t,
                                self.layout.active_tab_id() == Some(tab.id),
                                bells.contains(&tab.id),
                                unread.contains(&tab.id),
                            ))
                            .size(px(t.metrics.tab_icon_size))
                            .into_any_element(),
                        status: None,
                        bell: bells.contains(&tab.id),
                        unread: unread.contains(&tab.id),
                        closeable: true,
                        draggable: true,
                        detachable: false,
                    }
                })
                .collect(),
            active: self.layout.active_tab_id(),
            supports_detach: false,
            background: rgb(t.ui.shell_bg()),
            divider: self.chrome_divider(),
            active_background: rgb(t.ui.shell_active_tab_bg()),
            idle_background: rgb(t.ui.shell_bg()),
            translucent: false,
            leading_margin: 0.,
            filler: None,
            quick_actions,
            quick_actions_width,
            new_tab_label: self.ready.then(|| self.i18n.t("menu.new_terminal")),
        }
    }
    fn tab_request(&mut self, r: ShellTabRequest, w: &mut Window, cx: &mut Context<Self>) {
        match r {
            ShellTabRequest::Select(id) | ShellTabRequest::DoubleClick(id) => {
                self.layout.select_main_tab(Some(id));
                if let Some(s) = self.active_session().and_then(|s| self.sessions.get(&s)) {
                    w.focus(&s.view.read(cx).focus_handle(cx), cx);
                } else {
                    w.focus(&self.focus, cx);
                }
            }
            ShellTabRequest::NewTab => self.action(Action::Terminal, w, cx),
            ShellTabRequest::CloseTab { tab, .. } => self.close(tab, cx),
            ShellTabRequest::Reorder(id, index) => {
                self.layout.move_main_tab_to_visible_index(id, index);
            }
            ShellTabRequest::ShowTitle(id, title, p) => {
                self.hint(format!("tab-{}", id.0), Some((title, p)), cx)
            }
            ShellTabRequest::HideTitle(id) => self.hint(format!("tab-{}", id.0), None, cx),
            _ => {}
        }
        self.changed(cx);
    }
    fn hint(&mut self, id: String, data: Option<(String, Point<Pixels>)>, cx: &mut Context<Self>) {
        if let Some((label, p)) = data {
            self.transients
                .queue_tooltip(id, label, p.x.into(), p.y.into(), Instant::now());
        } else {
            self.transients.clear_tooltip(&id, Instant::now());
        }
        self.arm_deadline(cx);
    }
    fn arm_deadline(&mut self, cx: &mut Context<Self>) {
        self.deadline = self.transients.next_deadline().map(|d| {
            at_deadline(d, cx, |h, now, cx| {
                h.transients.process_due_deadlines(now);
                cx.notify();
                h.arm_deadline(cx);
            })
        });
    }
}
const FORM: u16 = 70;
const QUICK: u16 = 80;
const PALETTE: u16 = 90;
fn dark(w: &Window) -> bool {
    matches!(
        w.appearance(),
        WindowAppearance::Dark | WindowAppearance::VibrantDark
    )
}
impl ShellHost for Workspace {
    fn request(&mut self, r: ShellRequest, w: &mut Window, cx: &mut Context<Self>) {
        match r {
            ShellRequest::ExitZen => self.toggle_zen(cx),
            ShellRequest::HoverZenStrip(hovered) => {
                self.zen_exit.set_hot_zone_hovered(hovered);
                self.zen_hover(hovered, cx);
            }
            ShellRequest::HoverZenPill(hovered) => {
                self.zen_exit.set_pill_hovered(hovered);
                self.zen_hover(hovered, cx);
            }
            ShellRequest::Toggle(side) => {
                let close = !self.rails.rail_mut(side).collapsed();
                self.collapse(side, close, cx);
            }
            ShellRequest::Select(section) => {
                let close = self.rails.section == section && !self.rails.left.collapsed();
                if let Some(transition_gen) = self.rails.select_section(section, &self.tokens) {
                    let delay = settle_duration(&self.tokens, MotionDuration::Micro);
                    at_deadline(Instant::now() + delay, cx, move |h, _, cx| {
                        if h.rails.finish_section_transition(transition_gen) {
                            cx.notify();
                        }
                    })
                    .detach();
                }
                self.collapse(RailSide::Left, close, cx);
            }
            ShellRequest::BeginResize(side, _) => {
                let rail = self.rails.rail_mut(side);
                rail.begin_resize(rail.width(), &self.tokens);
            }
            ShellRequest::DragResize(x) => {
                if let Some(side) = self.rails.resizing_side() {
                    let width = side.width_at(
                        x.into(),
                        w.viewport_size().width.into(),
                        self.tokens.metrics.activity_bar_width,
                    );
                    let (min, max) = match side {
                        RailSide::Left => (
                            self.tokens.metrics.sidebar_min_width,
                            self.tokens.metrics.sidebar_max_width,
                        ),
                        RailSide::Right => (
                            oxideterm_settings::AI_SIDEBAR_ABSOLUTE_MIN_WIDTH,
                            oxideterm_settings::AI_SIDEBAR_ABSOLUTE_MAX_WIDTH,
                        ),
                    };
                    let width = clamp_responsive_sidebar_width(
                        width,
                        w.viewport_size().width.into(),
                        min,
                        max,
                    );
                    self.rails
                        .rail_mut(side)
                        .set_width(width, true, &self.tokens);
                }
            }
            ShellRequest::EndResize => {
                if let Some(side) = self.rails.resizing_side() {
                    self.rails.rail_mut(side).end_resize();
                } else {
                    return;
                }
            }
            ShellRequest::ShowToggleHint(side, label, p) => {
                self.hint(side.toggle_id().into(), Some((label, p)), cx)
            }
            ShellRequest::HideToggleHint(side) => self.hint(side.toggle_id().into(), None, cx),
        }
        self.changed(cx)
    }
}
impl PaneHost for Workspace {
    fn mount_pane(
        &mut self,
        tab: TabId,
        _: PaneId,
        c: PaneContent,
        _: bool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<MountedPane> {
        Some(MountedPane {
            title: self.layout.tab_by_id(tab)?.title.clone(),
            body: match c {
                PaneContent::Terminal(s) => self.sessions.get(&s)?.view.clone().into_any_element(),
                PaneContent::Page(t) => self.page(self.layout.tab_by_id(t)?, cx),
            },
            capture_focus: true,
            toolbar: None,
            overlays: vec![],
        })
    }
    fn pane_request(
        &mut self,
        r: ShellPaneRequest,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        use ShellPaneRequest::*;
        match r {
            Focus(t, p, c) => {
                self.sidebar_search_focused = false;
                self.layout.set_active_pane(Some(t), p);
                if let PaneContent::Terminal(s) = c
                    && let Some(s) = self.sessions.get(&s)
                {
                    w.focus(&s.view.read(cx).focus_handle(cx), cx);
                }
            }
            ClosePane(t, p, PaneContent::Terminal(s)) => {
                if self
                    .layout
                    .tab_by_id(t)
                    .and_then(|t| t.root_pane.as_ref())
                    .is_some_and(|r| r.pane_count() == 1)
                {
                    self.close(t, cx)
                } else {
                    self.layout.close_pane(t, p);
                    self.release_session(s, cx);
                }
            }
            ToggleZoom(t, p) => {
                self.layout.toggle_pane_zoom(t, p);
            }
            Swap(t, a, b) => {
                self.layout.swap_panes(t, a, b);
            }
            Resize(t, p, s) => {
                self.layout.update_group_sizes(Some(t), p, &s);
            }
            StartResize(d) => self.drag = Some(d),
            Reset(t, p) => {
                self.layout.reset_group_sizes(Some(t), p);
            }
            MoveOut(t, p, PaneContent::Terminal(s), false) => {
                let Some(mut new) = self.layout.tab_by_id(t).cloned() else {
                    return false;
                };
                new.id = self.layout.alloc_tab_id();
                new.active_pane_id = Some(p);
                new.root_pane = Some(PaneNode::leaf(p, s));
                self.layout.move_terminal_pane_to_tab(t, p, new);
            }
            _ => return false,
        }
        self.changed(cx);
        true
    }
}
fn notification_frame(
    tokens: ThemeTokens,
    i: &I18n,
    n: &NotificationState,
    host: WeakEntity<Workspace>,
) -> NotificationFrame {
    NotificationFrame {
        tokens,
        i18n: i.clone(),
        notifications: n.clone(),
        active_view: ActivityView::Notifications,
        has_background: false,
        transition: false,
        actions_available: false,
        request: Rc::new(move |r, _, cx| {
            let _ = host.update(cx, |h, cx| {
                use NotificationRequest::*;
                match r {
                    MarkRead(id) => h.notifications.mark_read(id),
                    MarkAllRead => h.notifications.mark_all_read(),
                    ToggleDnd => h.notifications.toggle_dnd(),
                    CycleStatus => h.notifications.cycle_status_filter(),
                    CycleSeverity => h.notifications.cycle_severity_filter(),
                    CycleKind => h.notifications.cycle_kind_filter(),
                    Clear => h.notifications.clear(),
                    Dismiss(id) => h.notifications.remove(id),
                    SelectView(_) | InvokeAction(_) => return,
                }
                h.refresh_center(cx);
                cx.notify();
            });
        }),
    }
}
