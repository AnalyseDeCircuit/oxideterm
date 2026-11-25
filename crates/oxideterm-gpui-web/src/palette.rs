use crate::host::{Action, Workspace};
use gpui::{prelude::*, *};
use oxideterm_gpui_connections::session_manager::SessionManagerOpenTarget;
use oxideterm_gpui_settings_view::SettingsTab;
use oxideterm_gpui_shell::{
    RailSide, ShellHost, ShellRequest, SidebarSection,
    chrome::tab_icon,
    palette::{model::*, *},
};
use oxideterm_gpui_ui::{
    icons::LucideIcon,
    motion::{MotionDuration, exit_settle_duration},
    text_input::TextInputViewport,
};
use oxideterm_settings::{CursorStyle as SettingsCursorStyle, PersistedSettings};
use oxideterm_workspace::{TabId, TabKind, parse_command_palette_query};
use std::rc::Rc;

#[derive(Clone)]
pub enum PaletteAction {
    Command(Action),
    ToggleLeftSidebar,
    SelectSidebar(SidebarSection),
    ActivateTab(TabId),
    NextTab(isize),
    CloseOtherTabs,
    CloseAllTabs,
    Connect(String),
    QuickConnect {
        username: String,
        host: String,
        port: u16,
    },
    FontAdjust(i32),
    FontReset,
    CursorStyle(SettingsCursorStyle),
    DisconnectAll,
    HealthCheck,
    OpenTopology,
    SettingsTab(SettingsTab),
    ResetSettings,
    ReloadWindow,
}

struct CommandSpec {
    id: &'static str,
    label_key: &'static str,
    icon: LucideIcon,
    shortcut_action: Option<&'static str>,
    action: PaletteAction,
}

pub struct Palette {
    pub model: Entity<PaletteModel<()>>,
    pub view: Entity<ShellPalette<PaletteAction>>,
    viewport: TextInputViewport,
}

impl Palette {
    pub fn new(cx: &mut Context<Workspace>) -> Self {
        let model = cx.new(|_| PaletteModel::new(()));
        let host = cx.weak_entity();
        let dispatch = host.clone();
        let view = cx.new(|cx| {
            ShellPalette::new(
                &model,
                move |_, cx| {
                    host.read_with(cx, |h, cx| {
                        let m = h.palette.model.read(cx);
                        let palette = m.view();
                        let ranked_items = h.ranked(cx);
                        let selected = m.selected_index();
                        let selected_index =
                            resolve_command_palette_selected_index(&ranked_items, selected);
                        let preview_selected = selected_index.is_some_and(|idx| {
                            ranked_items[idx].item.section == PaletteSection::Connections
                        });
                        PaletteFrame {
                            tokens: h.tokens,
                            i18n: h.i18n.clone(),
                            mono_font: "Lilex".into(),
                            selected,
                            preview_selected,
                            titlebar_height: h.tokens.metrics.titlebar_height,
                            scope_counts: None,
                            input: query_input(
                                &h.tokens,
                                &h.i18n,
                                &palette,
                                &h.palette.viewport,
                                true,
                                None,
                                None,
                                None,
                            )
                            .into_any_element(),
                            ranked_items,
                            palette,
                        }
                    })
                    .ok()
                },
                Rc::new(move |r, w, cx| {
                    let _ = dispatch.update(cx, |h, cx| h.palette_request(r, w, cx));
                }),
                cx,
            )
        });
        Self {
            view,
            model,
            viewport: Default::default(),
        }
    }

    pub fn open(&self, focus: &FocusHandle, w: &mut Window, cx: &mut Context<Workspace>) {
        self.model.update(cx, |m, cx| {
            m.open_content(PaletteContent::Commands, cx);
            m.focus.enter(focus, w, cx);
        });
    }
}

fn parse_quick_connect(query: &str) -> Option<(String, String, u16)> {
    if query.is_empty() || query.contains(char::is_whitespace) {
        return None;
    }
    let trimmed = query.strip_prefix("ssh://").unwrap_or(query);
    let (username, host_port) = trimmed.split_once('@')?;
    if username.is_empty() || host_port.is_empty() {
        return None;
    }
    let (host, port) = if let Some((h, p)) = host_port.rsplit_once(':') {
        if h.is_empty() {
            return None;
        }
        (h, p.parse::<u16>().ok()?)
    } else {
        (host_port, 22)
    };
    if host.is_empty() {
        return None;
    }
    Some((username.to_string(), host.to_string(), port))
}

fn tab_detail_key(kind: &TabKind) -> &'static str {
    match kind {
        TabKind::Workspace => "tabbar.workspace",
        TabKind::LocalTerminal => "layout.empty.new_local_terminal",
        TabKind::SshTerminal => "command_palette.session_ssh_terminal",
        TabKind::MoshTerminal => "terminal.typeMosh",
        TabKind::Settings => "settings_view.title",
        TabKind::Onboarding => "onboarding.welcome",
        TabKind::SessionManager => "sidebar.panels.saved_connections",
        TabKind::Runtime => "sidebar.panels.runtime",
        TabKind::ConnectionPool => "sidebar.panels.runtime_overview",
        TabKind::Topology => "topology.title",
        TabKind::NotificationCenter => "sidebar.panels.notifications",
        TabKind::PluginManager => "plugin.manager_title",
        TabKind::Plugin { .. } => "sidebar.panels.plugins",
        TabKind::CloudSync => "plugin.cloud_sync.panel_title",
        TabKind::Knowledge => "sidebar.panels.knowledge",
        TabKind::RemoteDesktop => "settings_view.terminal.bg_tab_remote_desktop",
        TabKind::Forwards => "sidebar.panels.forwarding",
        TabKind::Sftp => "sidebar.panels.sftp",
        TabKind::Ide => "settings_view.tabs.ide",
        TabKind::FileManager => "settings_view.help.category_file_manager",
        TabKind::Graphics => "settings_view.tabs.graphics",
    }
}

fn desktop_command_specs(demo: bool) -> Vec<CommandSpec> {
    let mut specs = vec![
        CommandSpec {
            id: "cmd:new_terminal",
            label_key: "command_palette.cmd_new_terminal",
            icon: LucideIcon::Terminal,
            shortcut_action: Some("app.newTerminal"),
            action: PaletteAction::Command(Action::Terminal),
        },
        CommandSpec {
            id: "cmd:new_connection",
            label_key: "command_palette.cmd_new_connection",
            icon: LucideIcon::Plus,
            shortcut_action: Some("app.newConnection"),
            action: PaletteAction::Command(Action::NewConnection),
        },
        CommandSpec {
            id: "cmd:open_ble_connection",
            label_key: "modals.new_connection.transport_ble",
            icon: LucideIcon::Bluetooth,
            shortcut_action: None,
            action: PaletteAction::Command(Action::Ble),
        },
        CommandSpec {
            id: "cmd:open_connection_manager",
            label_key: "command_palette.cmd_open_connection_manager",
            icon: LucideIcon::FolderOpen,
            shortcut_action: None,
            action: PaletteAction::Command(Action::Catalog),
        },
        CommandSpec {
            id: "cmd:settings",
            label_key: "command_palette.cmd_settings",
            icon: LucideIcon::Settings,
            shortcut_action: Some("app.settings"),
            action: PaletteAction::Command(Action::Settings),
        },
        CommandSpec {
            id: "cmd:toggle_sidebar",
            label_key: "command_palette.cmd_toggle_sidebar",
            icon: LucideIcon::PanelLeft,
            shortcut_action: Some("app.toggleSidebar"),
            action: PaletteAction::ToggleLeftSidebar,
        },
        CommandSpec {
            id: "cmd:split_horizontal",
            label_key: "command_palette.cmd_split_horizontal",
            icon: LucideIcon::SplitSquareHorizontal,
            shortcut_action: Some("split.horizontal"),
            action: PaletteAction::Command(Action::Split),
        },
        CommandSpec {
            id: "cmd:zen_mode",
            label_key: "command_palette.cmd_zen_mode",
            icon: LucideIcon::AppWindow,
            shortcut_action: Some("app.zenMode"),
            action: PaletteAction::Command(Action::Zen),
        },
        CommandSpec {
            id: "cmd:toggle_ai_sidebar",
            label_key: "command_palette.cmd_toggle_ai_sidebar",
            icon: LucideIcon::Bot,
            shortcut_action: Some("palette.aiSidebar"),
            action: PaletteAction::Command(Action::Assistant),
        },
        CommandSpec {
            id: "cmd:toggle_panel",
            label_key: "command_palette.cmd_toggle_panel",
            icon: LucideIcon::LayoutList,
            shortcut_action: Some("palette.eventLog"),
            action: PaletteAction::Command(Action::Notifications),
        },
        CommandSpec {
            id: "cmd:close_tab",
            label_key: "command_palette.cmd_close_tab",
            icon: LucideIcon::X,
            shortcut_action: Some("app.closeTab"),
            action: PaletteAction::Command(Action::Close),
        },
        CommandSpec {
            id: "cmd:next_tab",
            label_key: "command_palette.cmd_next_tab",
            icon: LucideIcon::ChevronRight,
            shortcut_action: Some("app.nextTab"),
            action: PaletteAction::NextTab(1),
        },
        CommandSpec {
            id: "cmd:prev_tab",
            label_key: "command_palette.cmd_prev_tab",
            icon: LucideIcon::ChevronLeft,
            shortcut_action: Some("app.prevTab"),
            action: PaletteAction::NextTab(-1),
        },
        CommandSpec {
            id: "cmd:close_other_tabs",
            label_key: "command_palette.cmd_close_other_tabs",
            icon: LucideIcon::Layers,
            shortcut_action: Some("app.closeOtherTabs"),
            action: PaletteAction::CloseOtherTabs,
        },
        CommandSpec {
            id: "cmd:close_all_tabs",
            label_key: "command_palette.cmd_close_all_tabs",
            icon: LucideIcon::Layers,
            shortcut_action: None,
            action: PaletteAction::CloseAllTabs,
        },
        CommandSpec {
            id: "cmd:theme_next",
            label_key: "command_palette.cmd_theme_next",
            icon: LucideIcon::Sparkles,
            shortcut_action: None,
            action: PaletteAction::Command(Action::Theme),
        },
        CommandSpec {
            id: "cmd:font_increase",
            label_key: "command_palette.cmd_font_increase",
            icon: LucideIcon::Plus,
            shortcut_action: Some("app.fontIncrease"),
            action: PaletteAction::FontAdjust(1),
        },
        CommandSpec {
            id: "cmd:font_decrease",
            label_key: "command_palette.cmd_font_decrease",
            icon: LucideIcon::ArrowDown,
            shortcut_action: Some("app.fontDecrease"),
            action: PaletteAction::FontAdjust(-1),
        },
        CommandSpec {
            id: "cmd:font_reset",
            label_key: "command_palette.cmd_font_reset",
            icon: LucideIcon::RotateCcw,
            shortcut_action: Some("app.fontReset"),
            action: PaletteAction::FontReset,
        },
        CommandSpec {
            id: "cmd:cursor_block",
            label_key: "command_palette.cmd_cursor_block",
            icon: LucideIcon::Square,
            shortcut_action: None,
            action: PaletteAction::CursorStyle(SettingsCursorStyle::Block),
        },
        CommandSpec {
            id: "cmd:cursor_bar",
            label_key: "command_palette.cmd_cursor_bar",
            icon: LucideIcon::Terminal,
            shortcut_action: None,
            action: PaletteAction::CursorStyle(SettingsCursorStyle::Bar),
        },
        CommandSpec {
            id: "cmd:cursor_underline",
            label_key: "command_palette.cmd_cursor_underline",
            icon: LucideIcon::ArrowDown,
            shortcut_action: None,
            action: PaletteAction::CursorStyle(SettingsCursorStyle::Underline),
        },
        CommandSpec {
            id: "cmd:sidebar_sessions",
            label_key: "command_palette.cmd_sidebar_sessions",
            icon: LucideIcon::ListTree,
            shortcut_action: None,
            action: PaletteAction::SelectSidebar(SidebarSection::Sessions),
        },
        CommandSpec {
            id: "cmd:sidebar_saved",
            label_key: "command_palette.cmd_sidebar_saved",
            icon: LucideIcon::Server,
            shortcut_action: None,
            action: PaletteAction::SelectSidebar(SidebarSection::Connections),
        },
    ];

    if demo {
        specs.extend([
            CommandSpec {
                id: "cmd:sidebar_sftp",
                label_key: "command_palette.cmd_sidebar_sftp",
                icon: LucideIcon::HardDrive,
                shortcut_action: None,
                action: PaletteAction::Command(Action::Sftp),
            },
            CommandSpec {
                id: "cmd:sidebar_forwards",
                label_key: "command_palette.cmd_sidebar_forwards",
                icon: LucideIcon::ArrowLeftRight,
                shortcut_action: None,
                action: PaletteAction::Command(Action::Forwards),
            },
            CommandSpec {
                id: "cmd:sidebar_ai",
                label_key: "command_palette.cmd_sidebar_ai",
                icon: LucideIcon::Bot,
                shortcut_action: None,
                action: PaletteAction::Command(Action::Assistant),
            },
            CommandSpec {
                id: "cmd:open_runtime",
                label_key: "command_palette.cmd_open_runtime",
                icon: LucideIcon::Gauge,
                shortcut_action: None,
                action: PaletteAction::Command(Action::Runtime),
            },
            CommandSpec {
                id: "cmd:open_topology",
                label_key: "command_palette.cmd_open_topology",
                icon: LucideIcon::Network,
                shortcut_action: None,
                action: PaletteAction::OpenTopology,
            },
            CommandSpec {
                id: "cmd:open_plugin_manager",
                label_key: "command_palette.cmd_open_plugin_manager",
                icon: LucideIcon::Puzzle,
                shortcut_action: None,
                action: PaletteAction::Command(Action::Plugins),
            },
            CommandSpec {
                id: "cmd:open_cloud_sync",
                label_key: "command_palette.cmd_open_cloud_sync",
                icon: LucideIcon::Cloud,
                shortcut_action: None,
                action: PaletteAction::Command(Action::CloudSync),
            },
            CommandSpec {
                id: "cmd:open_knowledge",
                label_key: "sidebar.panels.knowledge",
                icon: LucideIcon::BookOpen,
                shortcut_action: None,
                action: PaletteAction::Command(Action::Knowledge),
            },
            CommandSpec {
                id: "cmd:open_files",
                label_key: "sidebar.panels.files",
                icon: LucideIcon::FolderOpen,
                shortcut_action: None,
                action: PaletteAction::Command(Action::Files),
            },
            CommandSpec {
                id: "cmd:open_ide",
                label_key: "settings_view.tabs.ide",
                icon: LucideIcon::Code2,
                shortcut_action: None,
                action: PaletteAction::Command(Action::Ide),
            },
            CommandSpec {
                id: "cmd:quick_commands",
                label_key: "terminal.quick_commands.title",
                icon: LucideIcon::ListChecks,
                shortcut_action: None,
                action: PaletteAction::Command(Action::Quick),
            },
        ]);
    }

    specs.extend([
        CommandSpec {
            id: "cmd:disconnect_all",
            label_key: "command_palette.cmd_disconnect_all",
            icon: LucideIcon::Power,
            shortcut_action: None,
            action: PaletteAction::DisconnectAll,
        },
        CommandSpec {
            id: "cmd:health_check",
            label_key: "command_palette.cmd_health_check",
            icon: LucideIcon::Activity,
            shortcut_action: None,
            action: PaletteAction::HealthCheck,
        },
        CommandSpec {
            id: "cmd:reset_settings",
            label_key: "command_palette.cmd_reset_settings",
            icon: LucideIcon::AlertTriangle,
            shortcut_action: None,
            action: PaletteAction::ResetSettings,
        },
        CommandSpec {
            id: "cmd:reload_window",
            label_key: "command_palette.cmd_reload_window",
            icon: LucideIcon::RefreshCw,
            shortcut_action: None,
            action: PaletteAction::ReloadWindow,
        },
    ]);

    specs
}

fn desktop_help_specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "cmd:show_shortcuts",
            label_key: "command_palette.cmd_show_shortcuts",
            icon: LucideIcon::Keyboard,
            shortcut_action: Some("app.showShortcuts"),
            action: PaletteAction::SettingsTab(SettingsTab::Keybindings),
        },
        CommandSpec {
            id: "cmd:show_welcome",
            label_key: "command_palette.cmd_show_welcome",
            icon: LucideIcon::Home,
            shortcut_action: None,
            action: PaletteAction::Command(Action::Welcome),
        },
    ]
}

impl Workspace {
    fn spec_to_item(
        &self,
        spec: CommandSpec,
        section: PaletteSection,
        cx: &App,
    ) -> PaletteItem<PaletteAction> {
        let label = self.i18n.t(spec.label_key);
        let shortcut = spec.shortcut_action.and_then(|id| self.shortcut(id, cx));
        PaletteItem {
            id: spec.id.to_string(),
            value: format!("{label} {}", spec.id),
            label,
            section,
            icon: spec.icon,
            detail: None,
            shortcut,
            count: None,
            active: false,
            action: spec.action,
            disabled: false,
        }
    }

    fn quick_connect_item(&self, query: &str) -> Option<PaletteItem<PaletteAction>> {
        let (username, host, port) = parse_quick_connect(query)?;
        let target = if port == 22 {
            format!("{username}@{host}")
        } else {
            format!("{username}@{host}:{port}")
        };
        Some(PaletteItem {
            id: "quick_connect".to_string(),
            label: format!("{}: {target}", self.i18n.t("command_palette.quick_connect")),
            section: PaletteSection::QuickConnect,
            icon: LucideIcon::Zap,
            detail: None,
            shortcut: None,
            count: None,
            active: false,
            value: format!("quick_connect {target}"),
            action: PaletteAction::QuickConnect {
                username,
                host,
                port,
            },
            disabled: false,
        })
    }

    fn ranked(&self, cx: &App) -> Vec<RankedItem<PaletteAction>> {
        let raw_query = self.palette.model.read(cx).query();
        let (mode, query) = parse_command_palette_query(raw_query);
        let active_tab_id = self.layout.active_tab_id();

        let command_items: Vec<PaletteItem<PaletteAction>> = desktop_command_specs(self.demo)
            .into_iter()
            .map(|spec| self.spec_to_item(spec, PaletteSection::Commands, cx))
            .collect();

        let session_items: Vec<PaletteItem<PaletteAction>> = self
            .layout
            .main_tabs()
            .map(|(_, tab)| {
                let is_ble_tab = tab.root_pane.as_ref().is_some_and(|root| {
                    let mut ids = Vec::new();
                    root.collect_session_ids(&mut ids);
                    ids.iter().any(|sid| {
                        self.sessions
                            .get(sid)
                            .is_some_and(|s| crate::ble::is_ble_node(&s.node))
                    })
                });
                let detail = if is_ble_tab {
                    self.i18n.t("modals.new_connection.transport_ble")
                } else {
                    self.i18n.t(tab_detail_key(&tab.kind))
                };
                PaletteItem {
                    id: format!("session:{}", tab.id.0),
                    label: tab.title.clone(),
                    section: PaletteSection::Sessions,
                    icon: if is_ble_tab {
                        LucideIcon::Bluetooth
                    } else {
                        tab_icon(&tab.kind)
                    },
                    detail: Some(detail),
                    shortcut: None,
                    count: None,
                    active: active_tab_id == Some(tab.id),
                    value: tab.title.clone(),
                    action: PaletteAction::ActivateTab(tab.id),
                    disabled: false,
                }
            })
            .collect();

        let connection_items: Vec<PaletteItem<PaletteAction>> = self
            .catalog_data
            .items
            .iter()
            .filter_map(|row| {
                let SessionManagerOpenTarget::Connection(id) = &row.target else {
                    return None;
                };
                let is_ble = crate::ble::is_ble_node(id);
                let connected = self.sessions.values().any(|s| &s.node == id);
                let mut detail = row.subtitle.clone();
                for tag in &row.tags {
                    if !tag.is_empty() {
                        detail.push(' ');
                        detail.push_str(tag);
                    }
                }
                Some(PaletteItem {
                    id: format!("{}:{id}", if is_ble { "ble-profile" } else { "conn" }),
                    label: row.name.clone(),
                    section: PaletteSection::Connections,
                    icon: if is_ble {
                        LucideIcon::Bluetooth
                    } else {
                        LucideIcon::Server
                    },
                    detail: Some(detail.clone()),
                    shortcut: None,
                    count: None,
                    active: connected,
                    value: format!("{} {detail} {}", row.name, row.host),
                    action: PaletteAction::Connect(id.clone()),
                    disabled: false,
                })
            })
            .collect();

        let help_items: Vec<PaletteItem<PaletteAction>> = desktop_help_specs()
            .into_iter()
            .map(|spec| self.spec_to_item(spec, PaletteSection::Help, cx))
            .collect();

        let mru = &self.settings.read(cx).settings(cx).command_palette_mru;
        rank_commands(
            mode,
            &query,
            self.quick_connect_item(&query),
            mru,
            command_items,
            session_items,
            connection_items,
            Vec::new(),
            help_items,
        )
    }

    pub fn palette_key(&mut self, k: &Keystroke, w: &mut Window, cx: &mut Context<Self>) {
        let m = self.palette.model.read(cx);
        if let Some(r) = key_request(k, m.query(), m.selected_index(), || self.ranked(cx)) {
            self.palette_request(r, w, cx);
        }
    }

    fn palette_request(&mut self, r: ShellPaletteRequest, w: &mut Window, cx: &mut Context<Self>) {
        match r {
            ShellPaletteRequest::Execute(id) => {
                if let Some(action) = self
                    .ranked(cx)
                    .into_iter()
                    .find(|i| i.item.id == id)
                    .map(|i| i.item.action)
                {
                    if id != "quick_connect" {
                        let mru_id = id.clone();
                        self.settings.update(cx, |s, cx| {
                            s.edit_preferences(cx, |p| p.record_command_palette_use(&mru_id));
                        });
                    }
                    self.palette_request(ShellPaletteRequest::Close, w, cx);
                    match action {
                        PaletteAction::Command(a) => self.action(a, w, cx),
                        PaletteAction::ToggleLeftSidebar => {
                            self.request(ShellRequest::Toggle(RailSide::Left), w, cx);
                        }
                        PaletteAction::SelectSidebar(section) => {
                            self.request(ShellRequest::Select(section), w, cx);
                        }
                        PaletteAction::ActivateTab(tab_id) => {
                            self.layout.select_main_tab(Some(tab_id));
                            if let Some(s) =
                                self.active_session().and_then(|s| self.sessions.get(&s))
                            {
                                w.focus(&s.view.read(cx).focus_handle(cx), cx);
                            } else {
                                w.focus(&self.focus, cx);
                            }
                            self.changed(cx);
                        }
                        PaletteAction::NextTab(delta) => {
                            let ids: Vec<TabId> =
                                self.layout.main_tabs().map(|(_, t)| t.id).collect();
                            if ids.len() > 1 {
                                let cur =
                                    self.layout
                                        .active_tab_id()
                                        .and_then(|id| ids.iter().position(|c| *c == id))
                                        .unwrap_or(0) as isize;
                                let next = (cur + delta).rem_euclid(ids.len() as isize) as usize;
                                self.layout.select_main_tab(Some(ids[next]));
                                self.changed(cx);
                            }
                        }
                        PaletteAction::CloseOtherTabs => {
                            let active = self.layout.active_tab_id();
                            let others: Vec<TabId> = self
                                .layout
                                .main_tabs()
                                .map(|(_, t)| t.id)
                                .filter(|id| Some(*id) != active)
                                .collect();
                            for tab_id in others {
                                self.close(tab_id, cx);
                            }
                        }
                        PaletteAction::CloseAllTabs => {
                            let all: Vec<TabId> =
                                self.layout.main_tabs().map(|(_, t)| t.id).collect();
                            for tab_id in all {
                                self.close(tab_id, cx);
                            }
                        }
                        PaletteAction::Connect(node_id) => {
                            self.new_terminal(node_id, None, w, cx);
                        }
                        PaletteAction::QuickConnect {
                            username,
                            host,
                            port,
                        } => {
                            let target = format!("{username}@{host}:{port}");
                            self.new_terminal(target, None, w, cx);
                        }
                        PaletteAction::FontAdjust(delta) => {
                            self.settings.update(cx, |s, cx| {
                                s.edit_preferences(cx, |p| {
                                    let next = (p.terminal.font_size as i32 + delta).clamp(8, 32);
                                    p.terminal.font_size = next as i64;
                                });
                            });
                        }
                        PaletteAction::FontReset => {
                            let default_size = PersistedSettings::default().terminal.font_size;
                            self.settings.update(cx, |s, cx| {
                                s.edit_preferences(cx, |p| {
                                    p.terminal.font_size = default_size;
                                });
                            });
                        }
                        PaletteAction::CursorStyle(style) => {
                            self.settings.update(cx, |s, cx| {
                                s.edit_preferences(cx, |p| p.terminal.cursor_style = style);
                            });
                        }
                        PaletteAction::DisconnectAll => {
                            let nodes: Vec<String> =
                                self.sessions.values().map(|s| s.node.clone()).collect();
                            for node in nodes {
                                self.disconnect_node(&node, cx);
                            }
                        }
                        PaletteAction::HealthCheck => {
                            let total = self.sessions.len();
                            let healthy = if self.ready {
                                total
                            } else {
                                self.ble_links.len()
                            };
                            let msg = self
                                .i18n
                                .t("command_palette.health_result")
                                .replace("{{healthy}}", &healthy.to_string())
                                .replace("{{total}}", &total.to_string());
                            self.notifications_notice(msg, cx);
                        }
                        PaletteAction::OpenTopology => {
                            self.pages.runtime_section = crate::pages::RuntimeSection::Topology;
                            self.action(Action::Runtime, w, cx);
                        }
                        PaletteAction::SettingsTab(tab) => {
                            self.settings.update(cx, |s, cx| s.open_section(tab, cx));
                            self.action(Action::Settings, w, cx);
                        }
                        PaletteAction::ResetSettings => {
                            self.settings.update(cx, |s, cx| {
                                s.edit_preferences(cx, |p| *p = PersistedSettings::default());
                            });
                        }
                        PaletteAction::ReloadWindow => {
                            if let Some(win) = web_sys::window() {
                                let _ = win.location().reload();
                            }
                        }
                    }
                }
            }
            ShellPaletteRequest::Close => {
                let delay = exit_settle_duration(&self.tokens, MotionDuration::Control);
                self.palette.model.update(cx, |m, cx| {
                    m.begin_close(delay, cx);
                    m.focus.restore(&self.focus, w, cx);
                });
                self.changed(cx);
            }
            ShellPaletteRequest::Select(i) => {
                let items = self.ranked(cx);
                self.palette.model.update(cx, |m, cx| {
                    m.set_selected_index(i, cx);
                    m.reveal_selection(&items);
                });
            }
            ShellPaletteRequest::QueryChanged(q) => self.palette.model.update(cx, |m, cx| {
                let n = m.query().encode_utf16().count();
                m.replace_query_utf16(Some(0..n), &q, cx);
            }),
        }
    }
}
