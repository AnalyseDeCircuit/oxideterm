mod chrome;
mod notices;
pub(crate) use notices::NoticeTarget;
mod fields;
mod paste;
use fields::{Field, Fields};
mod confirm;
mod sender;
mod sidebar;
mod tab_drop;
mod tab_menu;
mod tab_scope;
use crate::{
    connections, demo::*, files::FileViews, palette::Palette, terminal::Session, tools::Tools,
    transport::*,
};
pub(crate) use confirm::CloseTarget;
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
        geometry::{ExitingTabVisual, TabStripItemKind, TabStripState, tab_exit_visual_index},
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
use oxideterm_settings_model::browser_preferences::WallpaperSettings;
use oxideterm_theme::ThemeTokens;
use oxideterm_workspace::*;
use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
    time::Duration,
};
pub(crate) use tab_scope::{BrowserTabs, Scope};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Terminal,
    TerminalSearch,
    Close,
    Split(SplitDirection),
    Palette,
    Settings,
    Catalog,
    Notifications,
    NewConnection,
    ImportConnections,
    Ble,
    Forwards,
    Quick,
    Welcome,
    Files,
    Sftp,
    Ide,
    Runtime,
    Topology,
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
            Self::TerminalSearch => "settings_view.keybindings.actions.terminal.search",
            Self::Close => "menu.close_tab",
            Self::Split(SplitDirection::Horizontal) => "menu.split_horizontal",
            Self::Split(SplitDirection::Vertical) => "menu.split_vertical",
            Self::Palette => "command_palette.title",
            Self::Settings => "menu.settings",
            Self::Catalog => "sidebar.panels.connections_header",
            Self::ImportConnections => "layout.empty.import_connections",
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
            Self::Topology => "topology.title",
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
    pub ui_font: SharedString,
    pub i18n: I18n,
    side: KeybindingSide,
    actions: Vec<ActionDefinition>,
    display_storage: crate::preferences::DisplayStorage,
    pub layout: BrowserTabs,
    rails: WorkspaceRails,
    strip: Entity<MeasuredTabStrip>,
    drag: Option<SplitDrag>,
    tab_drop: tab_drop::TabDrop,
    tab_menu: Option<(TabId, Entity<oxideterm_gpui_shell::tabs::menu::TabMenu>)>,
    confirmation: Option<confirm::Confirmation>,
    pub(super) paste_review: Option<paste::PasteReview>,
    zen_exit: zen::ZenExitAffordance,
    pub palette: Palette,
    sender: Entity<sender::WebSender>,
    transients: TransientOverlays<ToastContent, ()>,
    notice_targets: HashMap<u64, NoticeTarget>,
    deadline: Option<Task<()>>,
    notifications: NotificationState,
    center: Entity<NotificationCenter>,
    pub catalog: Entity<PortableCatalogView>,
    pub catalog_data: PortableCatalogData,
    pub(crate) catalog_storage: connections::storage::CatalogStorage,
    pub(crate) profile_details: HashMap<String, connections::ProfileDetails>,
    pub(crate) imports: connections::ConnectionImports,
    pub form: Option<Entity<PortableConnectionFormView>>,
    pub form_close_generation: u64,
    pub disclosure_motions: DisclosureMotions,
    pub segmented_motion: UserSegmentedControlMotionState,
    pub popover_generation: usize,
    pub sessions: HashMap<TerminalSessionId, Session>,
    pub paired_ble: Option<crate::ble::PairedWebBle>,
    pub(super) ble_scan: Option<Task<()>>,
    pub ble_profiles: HashMap<String, crate::ble::BleProfile>,
    pub expanded_nodes: HashSet<String>,
    session_host_view: bool,
    session_sort_order: oxideterm_settings::SessionSortOrder,
    session_sort_menu_open: bool,
    session_search_open: bool,
    fields: Fields,
    connections_sort_field: oxideterm_gpui_connections::session_manager::SessionSortField,
    connections_sort_direction: oxideterm_gpui_connections::session_manager::SortDirection,
    connections_sort_menu_open: bool,
    connections_search_open: bool,
    pub(super) selected_connection_id: Option<String>,
    connection_context_menu: Option<(String, Point<Pixels>)>,
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
    pub(crate) projects: crate::files::Projects,
    pub(crate) device_managers: crate::files::DeviceManagers,
    assistant: Entity<crate::ai::Assistant>,
    pub(crate) wallpaper: Option<WallpaperSettings>,
    pub(crate) wallpaper_library: crate::wallpaper::Library,
}
impl Workspace {
    pub fn new(
        side: KeybindingSide,
        gateway: Option<String>,
        saved: Result<connections::storage::Snapshot, ()>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let demo = gateway.is_none();
        let palette = Palette::new(demo, cx);
        let catalog = shortcuts::workspace_definitions();
        let actions: Vec<_> = catalog
            .iter()
            .filter(|a| palette.supports_shortcut(a, demo))
            .cloned()
            .collect();
        let (display_storage, preferences) = crate::preferences::DisplayStorage::load(catalog);
        let (terminal, editor) = crate::preferences::context_keys(&actions, &preferences, side);
        cx.set_global(terminal);
        cx.set_global(editor);
        let ui_font = browser_ui_font(&preferences.appearance.ui_font_family, cx);
        let sidebar = preferences.sidebar_ui.clone();
        let wallpaper = WallpaperSettings::active(&preferences.terminal).filter(|bg| {
            matches!(
                bg.image,
                oxideterm_settings_model::browser_preferences::WallpaperId::Bundled(_)
            )
        });
        let wallpaper_owner = cx.weak_entity();
        let mut layout = TabLayout::default();
        layout
            .configure_terminal_output_highlight(preferences.terminal.highlight_tab_on_new_output);
        let settings = cx.new(|cx| {
            PortableSettingsSurface::browser_workspace_with_wallpapers(
                preferences,
                actions.clone(),
                crate::wallpaper::gallery(demo, wallpaper_owner),
                cx,
            )
        });
        let report_owner = cx.weak_entity();
        settings.update(cx, |s, cx| {
            s.on_browser_report(
                move |w, cx| {
                    let _ = report_owner.update(cx, |h, cx| h.download_browser_report(w, cx));
                },
                cx,
            );
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
        let fields = Fields::new(tokens, &i18n, w, cx);
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
                let fs = DemoDrives::default();
                files = Some(FileViews::new(fs.clone(), tokens, &i18n, &ui_font, cx));
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
        let drop_owner = owner.clone();
        let strip = cx.new(|_| {
            MeasuredTabStrip::new(
                move |s, w, cx| owner.update(cx, |h, cx| h.strip_frame(s, w, cx)).ok(),
                move |req, w, cx| {
                    let _ = dispatch.update(cx, |h, cx| h.tab_request(req, w, cx));
                },
            )
            .with_content_drop(move |event, w, cx| {
                drop_owner
                    .update(cx, |h, cx| h.content_drop(event, w, cx))
                    .unwrap_or(false)
            })
        });
        let mut catalog_data = connections::catalog(demo);
        let mut profile_details = HashMap::new();
        let mut ble_profiles = HashMap::new();
        let baseline = match saved {
            Ok(snapshot) => {
                snapshot.clone().restore(
                    &mut catalog_data,
                    &mut profile_details,
                    files.as_mut().map(|f| &mut f.sftp_profiles),
                    &mut ble_profiles,
                );
                Some(snapshot)
            }
            Err(()) => None,
        };
        let catalog_storage = connections::storage::CatalogStorage::new(baseline);
        let catalog = cx.new(|cx| {
            PortableCatalogView::browser_workspace(
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
        let frame = notices::frame(
            tokens,
            &i18n,
            wallpaper
                .as_ref()
                .is_some_and(|bg| bg.includes("notification_center")),
            &notifications,
            &HashMap::new(),
            cx.weak_entity(),
        );
        let center = cx.new(|_| NotificationCenter::new(frame, None));
        let tools = Tools::new(demo, tokens, &i18n, &ui_font, &focus, cx);
        let host = cx.weak_entity();
        let sender = cx.new(|cx| sender::WebSender::new(host, tokens, &i18n, w, cx));
        let terminal_prefs = settings.read(cx).settings(cx).terminal.clone();
        sender.update(cx, |s, cx| {
            s.apply_appearance(tokens, &i18n, &terminal_prefs, cx)
        });
        let knowledge = crate::knowledge::WebKnowledge::new(&focus, demo.then_some(&settings), cx);
        let open_ai_settings_owner = cx.weak_entity();
        let insert_cmd_owner = cx.weak_entity();
        let assistant = {
            let settings_handle = settings.clone();
            cx.new(|cx| {
                crate::ai::Assistant::new(
                    tokens,
                    ui_font.clone(),
                    i18n.clone(),
                    settings_handle,
                    demo,
                    Rc::new(move |w, cx| {
                        let _ = open_ai_settings_owner.update(cx, |h, cx| {
                            h.settings.update(cx, |s, cx| {
                                s.open_ai_providers(cx);
                            });
                            h.action(Action::Settings, w, cx);
                        });
                    }),
                    Rc::new(move |command, window, cx| {
                        let _ = insert_cmd_owner.update(cx, |h, cx| {
                            h.insert_chat_command(command, window, cx);
                        });
                    }),
                    cx,
                )
            })
        };
        cx.subscribe(
            &assistant,
            |h, _, notice: &crate::ai::ChatSetupNotice, cx| h.notice(notice.0, cx),
        )
        .detach();
        Self {
            wallpaper,
            wallpaper_library: Default::default(),
            assistant,
            focus,
            settings,
            display_storage,
            ui_font,
            tokens,
            i18n,
            side,
            actions,
            layout,
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
                if demo {
                    ContextSidebarPanel::HostTools
                } else {
                    ContextSidebarPanel::Assistant
                },
            ),
            strip,
            drag: None,
            tab_drop: Default::default(),
            tab_menu: None,
            confirmation: None,
            paste_review: None,
            zen_exit: Default::default(),
            palette,
            transients: TransientOverlays::new(exit_settle_duration(
                &tokens,
                MotionDuration::Control,
            )),
            notice_targets: HashMap::new(),
            deadline: None,
            notifications,
            center,
            catalog,
            catalog_data,
            catalog_storage,
            profile_details,
            imports: Default::default(),
            form: None,
            form_close_generation: 0,
            disclosure_motions: DisclosureMotions::default(),
            segmented_motion: UserSegmentedControlMotionState::default(),
            popover_generation: 0,
            sessions: Default::default(),
            paired_ble: None,
            ble_scan: None,
            ble_profiles,
            expanded_nodes: HashSet::from(["demo".to_string(), "local".to_string()]),
            session_host_view: false,
            session_sort_order: oxideterm_settings::SessionSortOrder::Default,
            session_sort_menu_open: false,
            session_search_open: false,
            fields,
            connections_sort_field:
                oxideterm_gpui_connections::session_manager::SessionSortField::Name,
            connections_sort_direction:
                oxideterm_gpui_connections::session_manager::SortDirection::Asc,
            connections_sort_menu_open: false,
            connections_search_open: false,
            selected_connection_id: None,
            connection_context_menu: None,
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
            sender,
            files,
            projects: Default::default(),
            device_managers: Default::default(),
        }
    }
    pub fn start(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        w.focus(&self.focus, cx);
        self.load_wallpaper_list(cx);
        self.sync_wallpaper(cx);
        if self.catalog_storage.unavailable() {
            self.notice("sessionManager.browser_load_failed", cx);
        }
        if self.display_storage.restore_failed {
            self.notice("settings_view.appearance.browser_restore_failed", cx);
        }
        if self.settings.read(cx).settings(cx).sidebar_ui.zen_mode {
            self.rails.set_zen_mode(true, true, &self.tokens);
        }
        if self.demo {
            self.open_demo_preview(w, cx);
        }
        if !self.settings.read(cx).settings(cx).onboarding_completed {
            self.open_onboarding(cx);
        }
    }
    fn browser_preferences(
        &self,
        cx: &App,
    ) -> oxideterm_settings_model::browser_preferences::BrowserPreferences {
        let mut next = self
            .display_storage
            .capture(self.settings.read(cx).settings(cx));
        next.set_rails(
            (
                self.rails.left.width().round() as i64,
                self.rails.left.collapsed(),
            ),
            (
                self.rails.right.width().round() as i64,
                self.rails.right.collapsed(),
            ),
        );
        next
    }

    pub(crate) fn persist_onboarding(&mut self, enable_ai: bool, cx: &App) -> bool {
        let mut next = self.browser_preferences(cx);
        next.complete_onboarding(enable_ai);
        self.display_storage.persist(next).is_ok()
    }

    fn save_display(&mut self, cx: &mut Context<Self>) {
        let next = self.browser_preferences(cx);
        if self.display_storage.save(next) {
            self.notice("settings_view.appearance.browser_save_failed", cx);
        }
    }

    fn preferences(&mut self, w: &Window, cx: &mut Context<Self>) {
        self.save_display(cx);
        (self.tokens, self.i18n) = self.settings.read(cx).environment(dark(w), cx);
        self.ui_font = browser_ui_font(
            &self
                .settings
                .read(cx)
                .settings(cx)
                .appearance
                .ui_font_family,
            cx,
        );
        crate::icon::sync_browser_favicon(self.settings.read(cx).settings(cx).appearance.app_icon);
        self.sync_wallpaper(cx);
        let env = self.connection_env();
        self.catalog
            .update(cx, |v, cx| v.set_environment(env.clone(), cx));
        if let Some(f) = &self.form {
            f.update(cx, |v, cx| v.set_environment(env, cx));
        }
        self.layout.configure_terminal_output_highlight(
            self.settings
                .read(cx)
                .settings(cx)
                .terminal
                .highlight_tab_on_new_output,
        );
        self.layout.sync_tab_titles(|key| self.i18n.t(key));
        self.fields.environment(self.tokens, &self.i18n, cx);
        let settings = self.settings.read(cx).settings(cx);
        let (terminal, editor) =
            crate::preferences::context_keys(&self.actions, settings, self.side);
        cx.set_global(terminal);
        cx.set_global(editor);
        let prefs = self.settings.read(cx).settings(cx).terminal.clone();
        self.sender.update(cx, |s, cx| {
            s.apply_appearance(self.tokens, &self.i18n, &prefs, cx)
        });
        self.refresh_terminals(w, cx);
        App::notify(cx, self.tools.onboarding.entity_id());
        self.tools.forwards.refresh(cx);
        if let Some(v) = &self.tools.quick.views {
            App::notify(cx, v.manager.entity_id());
            App::notify(cx, v.dock.entity_id());
        }
        if let Some(f) = &self.files {
            f.refresh(self.tokens, &self.i18n, &self.ui_font, cx);
        }
        self.device_managers.refresh(cx);
        self.projects.appearance(
            self.tokens,
            &self.i18n,
            crate::files::file_editor_settings(self.settings.read(cx).settings(cx)),
            cx,
        );
        self.assistant.update(cx, |a, cx| {
            a.environment(self.tokens, self.i18n.clone(), self.ui_font.clone(), cx)
        });
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
    pub fn open_page(&mut self, kind: TabKind, key: &'static str, cx: &mut Context<Self>) {
        let existing = self
            .layout
            .tabs()
            .iter()
            .find(|t| t.kind == kind && self.device_managers.get(t.id).is_none())
            .map(|t| t.id);
        if let Some(id) = existing {
            self.activate_page(id, cx);
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
            self.sync_active_scope(cx);
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
        if self.tab_menu.as_ref().is_some_and(|(tab, _)| *tab == id) {
            self.tab_menu = None;
        }
        let Some(index) = self.layout.tab_index_by_id(id) else {
            return;
        };
        let exit_delay = exit_duration(&self.tokens, MotionDuration::Control);
        let exiting_visual = if self.tokens.motion.enabled && !exit_delay.is_zero() {
            let tab = &self.layout.tabs()[index];
            self.layout
                .main_tabs()
                .position(|(_, candidate)| candidate.id == tab.id)
                .and_then(|live_visual_index| {
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
                    Some(ExitingTabVisual {
                        tab_id: tab.id,
                        kind: tab.kind.clone(),
                        title: tab.title.clone(),
                        width: self.strip.read(cx).displayed_tab_width(id)?,
                        visual_index,
                        was_active: self.layout.active_tab_id() == Some(tab.id),
                        started_at: cx.background_executor().now(),
                        duration: exit_delay,
                    })
                })
        } else {
            None
        };
        let container = self.layout.container_tab_id(id);
        let mut pages = Vec::new();
        let mut sessions = Vec::new();
        if let Some(p) = &self.layout.tabs()[index].root_pane {
            p.collect_session_ids(&mut sessions);
            p.collect_page_ids(&mut pages);
        }
        for page in pages {
            if self
                .layout
                .tab_by_id(page)
                .is_some_and(|t| t.kind == TabKind::PluginManager)
            {
                self.pages.catalog.cancel();
            }
            self.tools.forwards.release(page);
            self.forget_notice_target(NoticeTarget::Page(page), cx);
            if let Some(idx) = self.layout.tab_index_by_id(page) {
                self.layout.remove_tab_at(idx);
            }
            self.projects.release(page);
            self.device_managers.release(page);
            if let Some(files) = &mut self.files {
                files.sftp_pages.remove(&page);
            }
        }
        if let Some(idx) = self.layout.tab_index_by_id(id) {
            self.layout.remove_tab_at(idx);
        }
        if !self
            .layout
            .tabs()
            .iter()
            .any(|t| t.kind == TabKind::PluginManager)
        {
            self.pages.catalog.cancel();
        }
        if container != id {
            self.tidy_container(container);
        }
        self.projects.release(id);
        self.device_managers.release(id);
        if let Some(files) = &mut self.files {
            files.sftp_pages.remove(&id);
        }
        self.tools.forwards.release(id);
        self.forget_notice_target(NoticeTarget::Page(id), cx);
        if let Some(visual) = exiting_visual {
            self.strip
                .update(cx, |strip, cx| strip.begin_exit(visual, cx));
        }
        self.transients
            .clear_tooltip(&format!("tab-{}", id.0), Instant::now());
        for s in sessions {
            self.release_session(s, cx);
        }
        self.reconcile_scope_after_close(cx);
        self.changed(cx);
    }
    pub(crate) fn request_close_terminals(
        &mut self,
        node: Option<&str>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let sessions: Vec<_> = self
            .sessions
            .iter()
            .filter(|(_, s)| node.is_none_or(|n| s.node == n))
            .map(|(id, _)| *id)
            .collect();
        if sessions.is_empty() {
            return;
        }
        let label = self.i18n.t("tabbar.close_terminals");
        let title = node.map_or(label.clone(), |n| format!("{label} · {n}"));
        self.request_close(CloseTarget::Terminals { sessions, title }, w, cx);
    }
    pub(super) fn action_available(&self, action: Action, cx: &App) -> bool {
        use self::Action::*;
        match action {
            Terminal => self.ready,
            TerminalSearch => self
                .active_session()
                .is_some_and(|id| self.sessions.contains_key(&id)),
            Split(_) => self.split_target().is_some(),
            Ble => crate::ble::available(),
            Assistant | Palette | Settings | Notifications | Welcome | Theme | Zen | Close
            | Plugins => true,
            Sftp => {
                self.demo
                    && (self.focused_tab().is_some_and(|t| t.kind == TabKind::Sftp)
                        || self.focused_file_node(cx).is_some())
            }
            Ide => self.focused_file_node(cx).is_some(),
            Forwards => self.focused_forward_node(cx).is_some(),
            Files => self.demo || crate::files::can_pick_directory(),
            ImportConnections | Catalog | NewConnection | Runtime | Topology | CloudSync
            | Knowledge | HostTool(_) | Quick => self.demo,
        }
    }

    pub fn action(&mut self, a: Action, w: &mut Window, cx: &mut Context<Self>) {
        use self::Action::*;
        if self.onboarding_locked() || !self.action_available(a, cx) {
            return;
        }
        match a {
            Terminal => self.new_terminal("local".into(), None, w, cx),
            TerminalSearch => {
                if let Some(s) = self.active_session().and_then(|id| self.sessions.get(&id)) {
                    s.view
                        .update(cx, |view, cx| view.open_search(&self.i18n, w, cx));
                }
            }
            Close => {
                if let Some(id) = self.layout.active_tab_id() {
                    self.request_close(CloseTarget::Tab(id), w, cx)
                }
            }
            Split(direction) => {
                if let Some(node) = self.split_target() {
                    self.new_terminal(node.to_owned(), Some(direction), w, cx)
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
            Welcome => self.open_onboarding(cx),
            Ble => self.form_for_transport(
                oxideterm_gpui_connections::form_state::NewConnectionTransport::Ble,
                w,
                cx,
            ),
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
            ImportConnections => self.open_ssh_import(w, cx),
            NewConnection => self.form(None, None, w, cx),
            Files => self.open_file_manager(cx),
            Sftp => {
                if self.focused_tab().is_some_and(|t| t.kind == TabKind::Sftp) {
                    return;
                }
                if let Some(node) = self.focused_file_node(cx) {
                    self.open_node_sftp(&node, cx);
                }
            }
            Ide => {
                if let Some(node) = self.focused_file_node(cx) {
                    self.open_node_ide(&node, w, cx);
                }
            }
            Forwards => {
                if let Some(node) = self.focused_forward_node(cx) {
                    self.open_node_forwards(&node, cx);
                }
            }
            Runtime => self.open_page(TabKind::Runtime, a.key(), cx),
            Topology => self.open_page(TabKind::Topology, a.key(), cx),
            Plugins => {
                self.open_page(TabKind::PluginManager, a.key(), cx);
                if self.pages.plugin_tab == crate::pages::PluginManagerTab::Marketplace {
                    self.ensure_plugin_catalog(cx);
                }
            }
            CloudSync => self.open_page(TabKind::CloudSync, a.key(), cx),
            Knowledge => {
                self.open_page(TabKind::Knowledge, a.key(), cx);
                if let Some(tab) = self
                    .layout
                    .tabs()
                    .iter()
                    .find(|t| t.kind == TabKind::Knowledge)
                {
                    self.knowledge.views.register_tab(tab.id, cx);
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
    pub(crate) fn plugin_search(&self) -> &Entity<oxideterm_gpui_editor::line_edit::LineField> {
        self.fields.view(Field::Marketplace)
    }
    pub fn frame(&mut self, w: &mut Window, cx: &mut Context<Self>) -> ShellContent {
        self.remember_file_terminal(cx);
        self.sync_sender_directory(cx);
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
            chrome_locked: self.form.is_some()
                || self.imports.is_open()
                || self.palette.model.read(cx).is_open()
                || self.paste_review.is_some()
                || self.onboarding_locked()
                || self.knowledge.dialog_open()
                || self.sftp_choice_open()
                || self.confirmation.is_some(),
            chrome_background: self.chrome_fill(t.ui.shell_bg()),
            divider: border,
            left_divider: self.divider(0x40),
        };
        self.projects.sync_project_mounts(&self.layout, cx);
        self.tab_drop.regions.borrow_mut().clear();
        let center = if let Some(tab) = self.layout.active_tab().cloned() {
            let content = if let Some(root) = tab.root_pane {
                PaneTree {
                    tab: tab.id,
                    active: tab.active_pane_id,
                    zoomed: self.layout.zoomed_pane(tab.id),
                    drag: self.drag.clone(),
                    supports_detach: false,
                    drop_regions: Some(self.tab_drop.regions.clone()),
                    tokens: t,
                    i18n: &self.i18n.clone(),
                    header_background: self.chrome_fill(t.ui.shell_bg()),
                    divider: border,
                }
                .render(&root, self, w, cx)
            } else {
                fade_in(
                    &t,
                    ("web-page-enter", tab.id.0 as usize),
                    div()
                        .size_full()
                        .child(self.wallpaper_content(&tab.kind, self.page(&tab, cx))),
                    MotionDuration::Micro,
                )
            };
            let owner = cx.weak_entity();
            measure_drop_region(
                tab.id,
                None,
                content,
                self.tab_drop.regions.clone(),
                move |w, cx| {
                    let _ = owner.update(cx, |h, cx| h.refresh_tab_drop(w, cx));
                },
            )
        } else if *self.layout.scope() != Scope::All {
            self.empty_scope(cx)
        } else {
            slide_fade_in_y(
                &t,
                "web-empty-workspace-enter",
                div().size_full().child({
                    let welcome = self.empty_workspace(w, cx);
                    if self.window_wallpaper().is_some() {
                        oxideterm_gpui_ui::tab_canvas_surface(&t, welcome)
                    } else {
                        welcome
                    }
                }),
                8.0,
                MotionDuration::Control,
            )
        };
        let center = self.with_sender(center, cx);
        let top = t.metrics.titlebar_height;
        let embedded = self
            .files_navigator_visible(cx)
            .then(|| self.embedded_files(cx))
            .flatten();
        let left = self.rails.left.mounted().then(|| {
            let drawer = self.left_drawer(cx);
            match &embedded {
                Some(browser) => self.with_embedded_files(drawer, browser, cx),
                None => drawer,
            }
        });
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
            .font_family(self.ui_font.clone())
            .text_color(rgb(t.ui.text))
            .bg(bg)
            .capture_key_down(cx.listener(Self::key))
            .on_mouse_move(cx.listener(|h, e: &MouseMoveEvent, w, cx| {
                h.resize_sender(Some(e), cx);
                h.resize_embedded_files(
                    e.pressed_button
                        .filter(|b| *b == MouseButton::Left)
                        .map(|_| e.position.y),
                    cx,
                );
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
                    h.resize_sender(None, cx);
                    h.resize_embedded_files(None, cx);
                    h.request(ShellRequest::EndResize, w, cx)
                }),
            );
        let mut overlays = OverlayStack::default();
        if self.assistant_visible() {
            overlays.at(PALETTE - 1);
            overlays.extend([self.assistant.read(cx).layers()]);
        }
        if let Some(browser) = embedded {
            overlays.at(FORM - 1);
            let editor = crate::files::file_editor_settings(self.settings.read(cx).settings(cx));
            overlays.extend([browser.update(cx, |b, cx| b.mount_dialog(editor, cx))]);
        }
        if let Some(choice) = self.sftp_choice(cx) {
            overlays.at(PALETTE + 1);
            overlays.extend([choice]);
        }
        if let Some(target) = self.tab_drop.preview {
            overlays.at(1);
            overlays.extend([target.preview(w, &t)]);
        }
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
        if let Some((_, menu)) = &self.tab_menu {
            overlays.at(65);
            overlays.extend([menu.clone().into_any_element()]);
        }
        if let Some(menu) = self.render_connection_context_menu(cx) {
            overlays.at(64);
            overlays.extend([menu]);
        }
        if let Some(menu) = self.topology_menu(w, cx) {
            overlays.at(65);
            overlays.extend([menu]);
        }
        if let Some(dialog) = self.render_import(w, cx) {
            overlays.at(PALETTE + 2);
            overlays.extend([dialog]);
        }
        if let Some(dialog) = self.render_paste_review(w, cx) {
            overlays.at(PALETTE + 2);
            overlays.extend([dialog]);
        }
        if let Some(dialog) = self.onboarding_legal_dialog(cx) {
            overlays.at(PALETTE + 2);
            overlays.extend([dialog]);
        }
        if let Some(dialog) = self.render_knowledge_dialog(w, cx) {
            overlays.at(PALETTE + 1);
            overlays.extend([dialog]);
        }
        if let Some(dialog) = self.rename_dialog(cx) {
            overlays.at(PALETTE + 1);
            overlays.extend([dialog]);
        }
        if let Some(dialog) = self.render_confirmation(cx) {
            overlays.at(PALETTE + 1);
            overlays.extend([dialog]);
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
                Clone::clone,
                |_| self.i18n.t("event_log.audit.notifications"),
                Rc::new(move |req, w, cx| {
                    let _ = owner.update(cx, |h, cx| {
                        let id = match req {
                            ToastRequest::Invoke(id) if h.transients.action(id).is_some() => {
                                h.action(Action::Notifications, w, cx);
                                id
                            }
                            ToastRequest::Dismiss(id) => id,
                            _ => return,
                        };
                        h.transients.begin_standard_exit(id, Instant::now());
                        h.arm_deadline(cx);
                        cx.notify();
                    });
                }),
            )
            .into_any_element()]);
        overlays.extend(self.transients.render_tooltip(&t));
        ShellContent::Frame(ShellFrameSlots {
            root,
            background: self
                .window_wallpaper()
                .and_then(|bg| self.wallpaper_layer(bg)),
            title_row: (!zen && !self.onboarding_locked()).then(|| self.titlebar(w, cx)),
            left_rail: Some(self.left_activity_rail(&env, cx)),
            right_rail: Some(self.right_activity_rail(w, &env, cx)),
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
            TabKind::Forwards => self.tools.forwards.page(tab.id),
            TabKind::Runtime => self.render_runtime_page(cx),
            TabKind::Topology => self.topology_page(cx),
            TabKind::PluginManager => self.render_plugin_manager_page(cx),
            TabKind::CloudSync => self.render_cloud_sync_page(cx),
            TabKind::Knowledge => self.knowledge.views.view.clone().into_any_element(),
            TabKind::Ide => self.projects.page(tab.id),
            TabKind::FileManager => self
                .manager_for(tab.id)
                .map(|v| {
                    div()
                        .relative()
                        .size_full()
                        .child(v.body.clone())
                        .child(v.dialog.clone())
                        .into_any_element()
                })
                .unwrap_or_else(|| div().into_any_element()),
            _ => self
                .files
                .as_ref()
                .map(|f| f.page(tab, cx))
                .unwrap_or_else(|| div().into_any_element()),
        }
    }
    pub(crate) fn matches_key(&self, key: &Keystroke, id: &str, cx: &App) -> bool {
        self.actions
            .iter()
            .find(|a| a.id == id)
            .is_some_and(|action| {
                shortcuts::keystroke_matches_action(
                    key,
                    action,
                    self.side,
                    &self.settings.read(cx).settings(cx).keybindings.overrides,
                )
            })
    }

    fn scoped_key(&self, key: &Keystroke, scope: ActionScope, cx: &App) -> Option<&str> {
        shortcuts::matched_scoped_action(
            &self.actions,
            self.side,
            key,
            scope,
            &self.settings.read(cx).settings(cx).keybindings.overrides,
        )
    }

    fn assistant_visible(&self) -> bool {
        !self.rails.right.collapsed() && self.rails.context_panel == ContextSidebarPanel::Assistant
    }

    fn key(&mut self, e: &KeyDownEvent, w: &mut Window, cx: &mut Context<Self>) {
        if self.onboarding_legal_key(e, cx)
            || self.onboarding_locked()
            || (self
                .focused_tab()
                .is_some_and(|t| t.kind == TabKind::Settings)
                && self.settings.read(cx).captures_workspace_shortcuts(cx))
        {
            return;
        }
        if self.imports.is_open() {
            self.import_key(&e.keystroke, w, cx);
            w.prevent_default();
            cx.stop_propagation();
            return;
        }
        if self.sftp_choice_open() {
            if e.keystroke.key == "escape" {
                self.choose_files_surface(None, cx);
            }
            w.prevent_default();
            cx.stop_propagation();
            return;
        }
        if self.paste_review.is_some() {
            return;
        }
        if self.knowledge.dialog_open() {
            self.knowledge_dialog_key(e, w, cx);
            return;
        }
        if e.keystroke.key == "escape" && self.pages.graph_menu.take().is_some() {
            cx.notify();
            cx.stop_propagation();
            w.prevent_default();
            return;
        }
        if self.tab_menu.is_some() {
            return;
        }
        if self.confirmation_key(&e.keystroke, w, cx)
            || self.context_tools_key(&e.keystroke.key, w, cx)
        {
            w.prevent_default();
            cx.stop_propagation();
            return;
        }
        if (self.session_sort_menu_open
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
        if self.assistant_visible() && self.assistant.update(cx, |a, cx| a.overlay_key(e, w, cx)) {
            return;
        }
        if self.assistant.read(cx).composer_focused(w, cx)
            || self.fields.owns_keys(w, cx)
            || (self.focused_manager().is_some_and(|v| {
                v.body.read(cx).field_focused(w, cx) || v.dialog.read(cx).field_focused(w, cx)
            }) && !matches!(e.keystroke.key.as_str(), "tab" | "up" | "down"))
        {
            return;
        }
        if self.knowledge_input_key(e, w, cx) {
            w.prevent_default();
            cx.stop_propagation();
            return;
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
        } else if self.files_own_keys(w, cx)
            || self.sender.read(cx).owns_keys(w, cx)
            || self.tools.forwards.editing_text(w)
        {
            return;
        } else if self.focused_manager().is_some_and(|manager| {
            let file = self.scoped_key(&e.keystroke, ActionScope::FileManager, cx);
            let preview = self.scoped_key(&e.keystroke, ActionScope::Preview, cx);
            manager.body.update(cx, |v, cx| {
                v.handle_file_manager_dialog_input_footer_key(e, cx)
                    || v.handle_file_manager_key(e, file, preview, cx)
            })
        }) {
        } else {
            let Some((def, _)) = shortcuts::matched_action_for_keystroke(
                &self.actions,
                self.side,
                &e.keystroke,
                &self.settings.read(cx).settings(cx).keybindings.overrides,
            ) else {
                return;
            };
            let id = def.id.as_ref();
            let terminal = self
                .active_session()
                .and_then(|id| self.sessions.get(&id))
                .map(|s| s.view.read(cx));
            if terminal.is_some_and(|v| v.search_focused(w, cx)) {
                if id != "terminal.search" {
                    return;
                }
            } else if id == "terminal.search"
                && !terminal.is_some_and(|v| v.focus_handle(cx).is_focused(w))
            {
                return;
            }
            if let Some(index) = id
                .strip_prefix("app.goToTab")
                .and_then(|n| n.parse::<usize>().ok())
                .and_then(|n| n.checked_sub(1))
            {
                if let Some(r) = shortcuts::TabShortcut::Index(index)
                    .request(&self.layout, self.layout.active_tab_id())
                {
                    self.tab_request(r, w, cx);
                } else {
                    return;
                }
            } else if let Some(action) = self.command_for_shortcut(id) {
                self.execute_command(action, w, cx);
            } else {
                return;
            }
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
                        self.ui_font.clone(),
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
            visuals: layout
                .items
                .iter()
                .map(|item| {
                    if let TabStripItemKind::Exiting(i) = item.kind {
                        let exit = &s.exiting_tabs[i];
                        return TabVisual {
                            title: exit.title.clone(),
                            icon: svg()
                                .path(oxideterm_gpui_shell::chrome::tab_icon(&exit.kind).path())
                                .text_color(TabVisual::color(&t, exit.was_active, false, false))
                                .size(px(t.metrics.tab_icon_size))
                                .into_any_element(),
                            status: None,
                            bell: false,
                            unread: false,
                            closeable: false,
                            draggable: false,
                            detachable: false,
                        };
                    }
                    let tab = self.layout.tab_by_id(item.tab_id).expect("measured tab");
                    let is_ble_tab = tab.root_pane.as_ref().is_some_and(|root| {
                        let mut ids = Vec::new();
                        root.collect_session_ids(&mut ids);
                        ids.iter()
                            .any(|sid| self.sessions.get(sid).is_some_and(|s| s.is_ble()))
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
                        closeable: !self.onboarding_locked(),
                        draggable: tab.kind != TabKind::Onboarding && !self.onboarding_locked(),
                        detachable: false,
                    }
                })
                .collect(),
            layout,
            active: self.layout.active_tab_id(),
            supports_detach: false,
            background: self.chrome_fill(t.ui.shell_bg()),
            divider: self.chrome_divider(),
            active_background: self.chrome_fill(t.ui.shell_active_tab_bg()),
            idle_background: if self.window_wallpaper().is_some() {
                rgba(0)
            } else {
                rgb(t.ui.shell_bg())
            },
            translucent: self.window_wallpaper().is_some(),
            leading_margin: 0.,
            filler: None,
            quick_actions,
            quick_actions_width,
            new_tab_label: (self.ready && !self.onboarding_locked())
                .then(|| self.i18n.t("menu.new_terminal")),
        }
    }
    pub(super) fn tab_request(
        &mut self,
        r: ShellTabRequest,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.onboarding_locked() {
            return;
        }
        match r {
            ShellTabRequest::DoubleClick(id) => self.begin_tab_rename(id, w, cx),
            ShellTabRequest::ContextMenu(id, pos) => self.open_tab_menu(id, pos, w, cx),
            ShellTabRequest::Select(id) => {
                self.activate_page(id, cx);
                if let Some(s) = self.active_session().and_then(|s| self.sessions.get(&s)) {
                    w.focus(&s.view.read(cx).focus_handle(cx), cx);
                } else {
                    w.focus(&self.focus, cx);
                }
            }
            ShellTabRequest::CancelDrag(id) => {
                self.tab_drop.preview = None;
                self.layout.select_main_tab(Some(id));
            }
            ShellTabRequest::DragChanged => {
                if self.strip.read(cx).state.drag.is_none() {
                    self.tab_drop.preview = None;
                }
            }
            ShellTabRequest::NewTab => self.action(Action::Terminal, w, cx),
            ShellTabRequest::CloseTab { tab, select_first } => {
                if select_first {
                    self.tab_request(ShellTabRequest::Select(tab), w, cx);
                }
                self.request_close(CloseTarget::Tab(tab), w, cx);
            }
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
fn browser_ui_font(configured: &str, cx: &App) -> SharedString {
    oxideterm_gpui_ui::typography::css_font_family_stack(configured)
        .into_iter()
        .find(|name| cx.text_system().font_id(&font(name.clone())).is_ok())
        .unwrap_or_else(|| "IBM Plex Sans".into())
}

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
                    self.save_display(cx);
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
        _: TabId,
        _: PaneId,
        c: PaneContent,
        _: bool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<MountedPane> {
        Some(MountedPane {
            title: match c {
                PaneContent::Page(id) => self.layout.tab_by_id(id)?.title.clone(),
                PaneContent::Terminal(id) => self.sessions.get(&id)?.title.clone(),
            },
            body: match c {
                PaneContent::Terminal(id) => {
                    let session = self.sessions.get(&id)?;
                    self.wallpaper_content(&session.kind(), session.view.clone().into_any_element())
                }
                PaneContent::Page(id) => {
                    let tab = self.layout.tab_by_id(id)?;
                    self.wallpaper_content(&tab.kind, self.page(tab, cx))
                }
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
                self.layout.set_active_pane(Some(t), p);
                if let PaneContent::Terminal(s) = c
                    && let Some(s) = self.sessions.get(&s)
                {
                    w.focus(&s.view.read(cx).focus_handle(cx), cx);
                }
            }
            ClosePane(t, p, PaneContent::Terminal(s)) => {
                self.request_close(CloseTarget::Pane(t, p, s), w, cx)
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
            Drop { source, target } => {
                let (direction, before) = target.edge.placement();
                return self.combine_tabs(
                    source,
                    target.tab,
                    target.pane,
                    direction,
                    before,
                    w,
                    cx,
                );
            }
            MoveOut(t, _, PaneContent::Page(page), false) => {
                if self.layout.container_tab_id(page) != t
                    || self.layout.unembed_page(page).is_none()
                {
                    return false;
                }
                self.tidy_container(t);
                self.activate_page(page, cx);
            }
            ClosePane(_, _, PaneContent::Page(page)) => {
                self.request_close(CloseTarget::Tab(page), w, cx)
            }
            MoveOut(t, p, PaneContent::Terminal(s), false) => {
                let Some(mut new) = self.layout.tab_by_id(t).cloned() else {
                    return false;
                };
                new.id = self.layout.alloc_tab_id();
                new.active_pane_id = Some(p);
                new.root_pane = Some(PaneNode::leaf(p, s));
                let Some(session) = self.sessions.get(&s) else {
                    return false;
                };
                new.kind = session.kind();
                new.title = session.title.clone();
                new.title_source = TabTitleSource::Static;
                let focus = session.view.read(cx).focus_handle(cx);
                let id = new.id;
                if !self.layout.move_terminal_pane_to_tab(t, p, new) {
                    return false;
                }
                self.tidy_container(t);
                self.activate_page(id, cx);
                w.focus(&focus, cx);
            }
            _ => return false,
        }
        self.changed(cx);
        true
    }
}
