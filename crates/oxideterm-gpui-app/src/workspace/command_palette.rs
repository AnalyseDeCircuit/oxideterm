use super::*;
use gpui_component::scroll::ScrollableElement;
use oxideterm_connections::{
    list_ssh_config_hosts, resolve_ssh_config_alias, saved_connection_from_ssh_host,
};
use oxideterm_gpui_settings_view::{OXIDE_THEME_IDS, built_in_theme_exists, is_oxide_theme};
use oxideterm_gpui_ui::modal::{dialog_backdrop, dialog_content};
use oxideterm_theme::BUILT_IN_THEMES;
use std::borrow::Cow;

const COMMAND_PALETTE_WIDTH: f32 = 560.0;
const COMMAND_PALETTE_FALLBACK_TOP: f32 = 96.0;
const COMMAND_PALETTE_TOP_RATIO: f32 = 0.15;
const COMMAND_PALETTE_LIST_MAX_HEIGHT: f32 = 400.0;
const COMMAND_PALETTE_ROW_HEIGHT: f32 = 40.0;
const COMMAND_PALETTE_BACKDROP_ALPHA: u32 = 0x66; // Tauri bg-black/40.
const COMMAND_PALETTE_SELECTED_ALPHA: u32 = 0x26; // Tauri accent/15.
const QUICK_CONNECT_REQUIRES_AT: char = '@';

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PaletteSection {
    QuickConnect,
    Recent,
    Commands,
    Sessions,
    Connections,
    #[allow(dead_code)]
    Plugins,
    Help,
}

#[derive(Clone)]
struct PaletteItem {
    id: String,
    label: String,
    section: PaletteSection,
    icon: LucideIcon,
    detail: Option<String>,
    shortcut: Option<String>,
    value: String,
    action: PaletteAction,
    disabled: bool,
}

#[derive(Clone)]
enum PaletteAction {
    Keybinding(&'static str),
    ActivateTab(TabId),
    OpenSavedConnection(String),
    QuickConnectHost {
        username: String,
        host: String,
        port: u16,
    },
    QuickConnectAlias(String),
    Sidebar(SidebarSection),
    OpenSessionManager,
    OpenConnectionPool,
    OpenConnectionMonitor,
    OpenTopology,
    OpenPluginManager,
    OpenNotificationCenter,
    CloseAllTabs,
    ThemeNext(bool),
    CursorStyle(SettingsCursorStyle),
    ToggleFps,
}

#[derive(Clone)]
struct CommandSpec {
    id: &'static str,
    label_key: Cow<'static, str>,
    icon: LucideIcon,
    shortcut_action: Option<&'static str>,
    action: PaletteAction,
}

#[derive(Clone)]
struct RankedItem {
    item: PaletteItem,
    score: f32,
    highlights: Vec<usize>,
}

impl WorkspaceApp {
    pub(super) fn open_command_palette(&mut self, cx: &mut Context<Self>) {
        self.command_palette.open = true;
        self.command_palette.raw_query.clear();
        self.command_palette.mode = PaletteMode::All;
        self.command_palette.selected_index = 0;
        self.command_palette.error = None;
        self.ime_marked_text = None;
        self.load_command_palette_ssh_config_hosts(cx);
        cx.notify();
    }

    pub(super) fn close_command_palette(&mut self, cx: &mut Context<Self>) {
        self.command_palette.open = false;
        self.command_palette.raw_query.clear();
        self.command_palette.mode = PaletteMode::All;
        self.command_palette.selected_index = 0;
        self.command_palette.error = None;
        self.ime_marked_text = None;
        cx.notify();
    }

    fn load_command_palette_ssh_config_hosts(&mut self, cx: &mut Context<Self>) {
        self.command_palette.ssh_config_hosts_loading = true;
        self.command_palette.error = None;
        let existing_names = self
            .connection_store
            .connections()
            .iter()
            .map(|conn| conn.name.clone())
            .collect::<HashSet<_>>();
        cx.spawn(async move |weak, cx| {
            let result = list_ssh_config_hosts(&existing_names).map_err(|error| error.to_string());
            let _ = weak.update(cx, |this, cx| {
                this.command_palette.ssh_config_hosts_loading = false;
                match result {
                    Ok(hosts) => {
                        this.command_palette.ssh_config_hosts = hosts;
                        this.command_palette.error = None;
                    }
                    Err(error) => {
                        this.command_palette.ssh_config_hosts.clear();
                        this.command_palette.error = Some(error);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn open_shortcuts_modal(&mut self, cx: &mut Context<Self>) {
        self.shortcuts_modal.open = true;
        self.shortcuts_modal.query.clear();
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn close_shortcuts_modal(&mut self, cx: &mut Context<Self>) {
        self.shortcuts_modal.open = false;
        self.shortcuts_modal.query.clear();
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn handle_command_palette_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        match key {
            "escape" if !event.keystroke.modifiers.platform => self.close_command_palette(cx),
            "enter" if !event.keystroke.modifiers.platform => {
                self.execute_selected_command_palette_item(window, cx);
            }
            "arrowdown" | "down" => {
                let count = self.filtered_command_palette_items().len();
                if count > 0 {
                    self.command_palette.selected_index =
                        (self.command_palette.selected_index + 1).min(count - 1);
                    cx.notify();
                }
            }
            "arrowup" | "up" => {
                self.command_palette.selected_index =
                    self.command_palette.selected_index.saturating_sub(1);
                cx.notify();
            }
            "backspace" if !event.keystroke.modifiers.platform => {
                self.command_palette.raw_query.pop();
                self.update_command_palette_mode_from_query(cx);
            }
            _ => {
                if let Some(text) = event.keystroke.key_char.as_deref()
                    && !event.keystroke.modifiers.platform
                    && !event.keystroke.modifiers.control
                    && !text.chars().any(char::is_control)
                {
                    self.command_palette.raw_query.push_str(text);
                    self.update_command_palette_mode_from_query(cx);
                }
            }
        }
    }

    fn update_command_palette_mode_from_query(&mut self, cx: &mut Context<Self>) {
        let (mode, _) = parse_command_palette_mode(&self.command_palette.raw_query);
        self.command_palette.mode = mode;
        self.command_palette.selected_index = 0;
        self.command_palette.error = None;
        cx.notify();
    }

    pub(super) fn handle_shortcuts_modal_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        match key {
            "escape" if !event.keystroke.modifiers.platform => self.close_shortcuts_modal(cx),
            "backspace" if !event.keystroke.modifiers.platform => {
                self.shortcuts_modal.query.pop();
                cx.notify();
            }
            _ => {
                if let Some(text) = event.keystroke.key_char.as_deref()
                    && !event.keystroke.modifiers.platform
                    && !event.keystroke.modifiers.control
                    && !text.chars().any(char::is_control)
                {
                    self.shortcuts_modal.query.push_str(text);
                    cx.notify();
                }
            }
        }
    }

    fn execute_selected_command_palette_item(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let items = self.filtered_command_palette_items();
        let Some(item) = items.get(self.command_palette.selected_index).cloned() else {
            return;
        };
        self.execute_command_palette_item(item, window, cx);
    }

    fn execute_command_palette_item(
        &mut self,
        item: PaletteItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if item.disabled {
            return;
        }
        self.record_command_palette_mru(&item.id);
        self.command_palette.open = false;
        self.command_palette.raw_query.clear();
        self.command_palette.mode = PaletteMode::All;
        self.command_palette.selected_index = 0;
        self.command_palette.error = None;
        self.ime_marked_text = None;

        match item.action {
            PaletteAction::Keybinding(action_id) => {
                let _ = self.dispatch_keybinding_action(action_id, window, cx);
            }
            PaletteAction::ActivateTab(tab_id) => self.set_active_tab(tab_id, window, cx),
            PaletteAction::OpenSavedConnection(connection_id) => {
                self.open_saved_connection_from_palette(connection_id, window, cx);
            }
            PaletteAction::QuickConnectHost {
                username,
                host,
                port,
            } => self.open_quick_connect_form(username, host, port, window, cx),
            PaletteAction::QuickConnectAlias(alias) => {
                self.open_ssh_config_alias_from_palette(alias, window, cx);
            }
            PaletteAction::Sidebar(section) => self.set_sidebar_section(section, cx),
            PaletteAction::OpenSessionManager => self.open_session_manager_tab(window, cx),
            PaletteAction::OpenConnectionPool => self.open_connection_pool_tab(window, cx),
            PaletteAction::OpenConnectionMonitor => self.open_connection_monitor_tab(window, cx),
            PaletteAction::OpenTopology => self.open_topology_tab(window, cx),
            PaletteAction::OpenPluginManager => self.open_plugin_manager_tab(window, cx),
            PaletteAction::OpenNotificationCenter => self.open_notification_center_tab(window, cx),
            PaletteAction::CloseAllTabs => self.close_all_tabs_from_palette(window, cx),
            PaletteAction::ThemeNext(forward) => self.step_terminal_theme(forward, cx),
            PaletteAction::CursorStyle(cursor_style) => {
                self.edit_settings(|settings| settings.terminal.cursor_style = cursor_style, cx);
            }
            PaletteAction::ToggleFps => {
                self.edit_settings(
                    |settings| {
                        settings.terminal.show_fps_overlay = !settings.terminal.show_fps_overlay;
                    },
                    cx,
                );
            }
        }
        cx.notify();
    }

    fn record_command_palette_mru(&mut self, id: &str) {
        let mru = &mut self.settings_store.settings_mut().command_palette_mru;
        mru.retain(|candidate| candidate != id);
        mru.insert(0, id.to_string());
        mru.truncate(20);
        let _ = self.settings_store.save();
    }

    fn close_all_tabs_from_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        while self.active_tab_id.is_some() {
            let before = self.tabs.len();
            self.close_active_tab(window, cx);
            if self.tabs.len() == before {
                break;
            }
        }
    }

    fn open_saved_connection_from_palette(
        &mut self,
        connection_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(conn) = self.connection_store.get(&connection_id).cloned() else {
            return;
        };
        let Some(config) =
            super::session_manager::ssh_config_from_saved_connection(&self.connection_store, &conn)
        else {
            self.open_saved_connection_prompt(
                &connection_id,
                SavedConnectionPromptAction::Connect,
                Some(
                    self.i18n
                        .t("sessionManager.edit_properties.password_placeholder"),
                ),
                window,
                cx,
            );
            return;
        };
        let _ = self.connection_store.mark_used(&connection_id);
        let _ = self.open_or_create_saved_ssh_terminal_tab(
            connection_id,
            config,
            conn.name.clone(),
            window,
            cx,
        );
    }

    fn open_quick_connect_form(
        &mut self,
        username: String,
        host: String,
        port: u16,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.prepare_modal_interaction_boundary();
        self.new_connection_form = Some(NewConnectionForm {
            name: host.clone(),
            host,
            port: port.to_string(),
            username,
            focused_field: NewConnectionField::Password,
            group: self.i18n.t("ssh.form.ungrouped"),
            ..NewConnectionForm::default()
        });
        self.drill_down_parent_node_id = None;
        self.editing_saved_connection_id = None;
        self.saved_connection_prompt_action = None;
        self.open_new_connection_select = None;
        self.new_connection_caret_visible = true;
        self.needs_active_pane_focus = false;
        window.focus(&self.focus_handle);
        cx.notify();
    }

    fn open_ssh_config_alias_from_palette(
        &mut self,
        alias: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match resolve_ssh_config_alias(&alias) {
            Ok(Some(host)) => match saved_connection_from_ssh_host(host) {
                Ok(conn) => {
                    self.prepare_modal_interaction_boundary();
                    self.new_connection_form = Some(
                        super::session_manager::form_from_saved_connection(&conn, None),
                    );
                    self.drill_down_parent_node_id = None;
                    self.editing_saved_connection_id = None;
                    self.saved_connection_prompt_action = None;
                    self.open_new_connection_select = None;
                    self.new_connection_caret_visible = true;
                    self.needs_active_pane_focus = false;
                    window.focus(&self.focus_handle);
                }
                Err(error) => self.command_palette.error = Some(error.to_string()),
            },
            Ok(None) => {
                self.command_palette.error =
                    Some(self.i18n.t("command_palette.quick_connect_alias_not_found"));
            }
            Err(error) => {
                let message = self.i18n.t("command_palette.quick_connect_resolve_failed");
                self.command_palette.error = Some(format!("{message}: {error}"));
            }
        }
        cx.notify();
    }

    fn step_terminal_theme(&mut self, forward: bool, cx: &mut Context<Self>) {
        let settings = self.settings_store.settings();
        let mut theme_ids = settings.custom_themes.keys().cloned().collect::<Vec<_>>();
        theme_ids.sort();
        for &theme_id in OXIDE_THEME_IDS {
            if built_in_theme_exists(theme_id) {
                theme_ids.push(theme_id.to_string());
            }
        }
        let mut classic = BUILT_IN_THEMES
            .iter()
            .filter(|theme| !is_oxide_theme(theme.id))
            .map(|theme| theme.id.to_string())
            .collect::<Vec<_>>();
        classic.sort();
        theme_ids.extend(classic);
        if theme_ids.is_empty() {
            return;
        }
        let current = self.settings_store.settings().terminal.theme.clone();
        let index = theme_ids
            .iter()
            .position(|candidate| candidate == &current)
            .unwrap_or(0);
        let next_index = if forward {
            (index + 1) % theme_ids.len()
        } else if index == 0 {
            theme_ids.len() - 1
        } else {
            index - 1
        };
        let next_theme = theme_ids[next_index].clone();
        self.edit_settings(|settings| settings.terminal.theme = next_theme, cx);
    }

    fn filtered_command_palette_items(&self) -> Vec<PaletteItem> {
        self.ranked_command_palette_items()
            .into_iter()
            .map(|ranked| ranked.item)
            .collect()
    }

    fn ranked_command_palette_items(&self) -> Vec<RankedItem> {
        let (mode, query) = parse_command_palette_mode(&self.command_palette.raw_query);
        let mut ranked = Vec::new();

        if mode == PaletteMode::All {
            if let Some(item) = self.quick_connect_item(&query) {
                ranked.push(RankedItem {
                    item,
                    score: 2.0,
                    highlights: Vec::new(),
                });
            }
        }

        let command_items = self.command_palette_command_items();
        let session_items = self.command_palette_session_items();
        let connection_items = self.command_palette_connection_items();
        let plugin_items = self.command_palette_plugin_items();
        let help_items = self.command_palette_help_items();

        if mode == PaletteMode::All && query.is_empty() {
            let mut by_id = HashMap::<String, PaletteItem>::new();
            for item in command_items
                .iter()
                .chain(session_items.iter())
                .chain(connection_items.iter())
                .chain(plugin_items.iter())
                .chain(help_items.iter())
            {
                by_id.insert(item.id.clone(), item.clone());
            }
            for id in self
                .settings_store
                .settings()
                .command_palette_mru
                .iter()
                .take(5)
            {
                if let Some(mut item) = by_id.get(id).cloned() {
                    item.section = PaletteSection::Recent;
                    ranked.push(RankedItem {
                        item,
                        score: 1.0,
                        highlights: Vec::new(),
                    });
                }
            }
        }

        if matches!(mode, PaletteMode::All | PaletteMode::Commands) {
            ranked.extend(rank_palette_section(command_items, &query));
        }
        if matches!(mode, PaletteMode::All | PaletteMode::Sessions) {
            ranked.extend(rank_palette_section(session_items, &query));
        }
        if matches!(mode, PaletteMode::All | PaletteMode::Connections) {
            ranked.extend(rank_palette_section(connection_items, &query));
        }
        if matches!(mode, PaletteMode::All | PaletteMode::Commands) {
            ranked.extend(rank_palette_section(plugin_items, &query));
            ranked.extend(rank_palette_section(help_items, &query));
        }

        ranked.truncate(80);
        ranked
    }

    fn quick_connect_item(&self, query: &str) -> Option<PaletteItem> {
        if query.is_empty() || query.contains(char::is_whitespace) {
            return None;
        }
        if let Some((username, host, port)) = parse_user_host_port(query) {
            return Some(PaletteItem {
                id: format!("quick-connect:{query}"),
                label: self.quick_connect_label(query),
                section: PaletteSection::QuickConnect,
                icon: LucideIcon::Zap,
                detail: Some(format!("{username}@{host}:{port}")),
                shortcut: None,
                value: query.to_string(),
                action: PaletteAction::QuickConnectHost {
                    username,
                    host,
                    port,
                },
                disabled: false,
            });
        }
        let alias = query.to_string();
        let matched_alias = self
            .command_palette
            .ssh_config_hosts
            .iter()
            .any(|host| host.alias.eq_ignore_ascii_case(&alias));
        if matched_alias {
            return Some(PaletteItem {
                id: format!("quick-connect-alias:{alias}"),
                label: self.quick_connect_label(&alias),
                section: PaletteSection::QuickConnect,
                icon: LucideIcon::Zap,
                detail: Some(self.i18n.t("command_palette.ssh_config_alias")),
                shortcut: None,
                value: alias.clone(),
                action: PaletteAction::QuickConnectAlias(alias),
                disabled: false,
            });
        }
        None
    }

    fn command_palette_command_items(&self) -> Vec<PaletteItem> {
        command_palette_specs()
            .into_iter()
            .map(|spec| self.command_palette_spec_item(spec, PaletteSection::Commands))
            .collect()
    }

    fn command_palette_help_items(&self) -> Vec<PaletteItem> {
        help_palette_specs()
            .into_iter()
            .map(|spec| self.command_palette_spec_item(spec, PaletteSection::Help))
            .collect()
    }

    fn command_palette_spec_item(&self, spec: CommandSpec, section: PaletteSection) -> PaletteItem {
        let label = self.i18n.t(spec.label_key.as_ref());
        let shortcut = spec.shortcut_action.and_then(|action_id| {
            crate::keybindings::action_definition(action_id).map(|definition| {
                crate::keybindings::format_combo(&crate::keybindings::effective_combo(
                    definition,
                    &self.settings_store.settings().keybindings.overrides,
                    crate::keybindings::KeybindingSide::current(),
                ))
            })
        });
        PaletteItem {
            id: spec.id.to_string(),
            label: label.clone(),
            section,
            icon: spec.icon,
            detail: None,
            shortcut,
            value: format!("{} {}", label, spec.id),
            action: spec.action,
            disabled: false,
        }
    }

    fn command_palette_session_items(&self) -> Vec<PaletteItem> {
        self.tabs
            .iter()
            .map(|tab| {
                let detail = match tab.kind {
                    TabKind::LocalTerminal => self.i18n.t("layout.empty.new_local_terminal"),
                    TabKind::SshTerminal => self.i18n.t("command_palette.session_ssh_terminal"),
                    TabKind::Settings => self.i18n.t("settings_view.title"),
                    TabKind::SessionManager => self.i18n.t("sidebar.panels.saved_connections"),
                    TabKind::ConnectionPool => self.i18n.t("sidebar.panels.connection_pool"),
                    TabKind::ConnectionMonitor => self.i18n.t("sidebar.panels.connection_monitor"),
                    TabKind::Topology => self.i18n.t("topology.title"),
                    TabKind::NotificationCenter => self.i18n.t("sidebar.panels.notifications"),
                    TabKind::PluginManager => self.i18n.t("plugin.manager_title"),
                    TabKind::Forwards => self.i18n.t("sidebar.panels.forwarding"),
                    TabKind::Sftp => self.i18n.t("sidebar.panels.sftp"),
                    TabKind::Ide => self.i18n.t("settings_view.tabs.ide"),
                    TabKind::FileManager => self.i18n.t("settings_view.help.category_file_manager"),
                    TabKind::Launcher => self.i18n.t("app.shellLauncher"),
                    TabKind::Graphics => self.i18n.t("settings_view.tabs.graphics"),
                };
                PaletteItem {
                    id: format!("session:{}", tab.id.0),
                    label: tab.title.clone(),
                    section: PaletteSection::Sessions,
                    icon: tab_kind_icon(&tab.kind),
                    detail: Some(detail),
                    shortcut: None,
                    value: format!("{} session tab {}", tab.title, tab.id.0),
                    action: PaletteAction::ActivateTab(tab.id),
                    disabled: false,
                }
            })
            .collect()
    }

    fn command_palette_connection_items(&self) -> Vec<PaletteItem> {
        self.connection_store
            .connections()
            .iter()
            .map(|conn| PaletteItem {
                id: format!("connection:{}", conn.id),
                label: conn.name.clone(),
                section: PaletteSection::Connections,
                icon: LucideIcon::Server,
                detail: Some(format!("{}@{}:{}", conn.username, conn.host, conn.port)),
                shortcut: None,
                value: format!(
                    "{} {} {} {}",
                    conn.name,
                    conn.host,
                    conn.username,
                    conn.group.as_deref().unwrap_or_default()
                ),
                action: PaletteAction::OpenSavedConnection(conn.id.clone()),
                disabled: false,
            })
            .collect()
    }

    fn command_palette_plugin_items(&self) -> Vec<PaletteItem> {
        // Tauri shows plugin commands supplied by the plugin command registry.
        // Native has a plugin manager placeholder but no executable command
        // registry yet, so keep this section empty instead of pretending that
        // commands can run.
        Vec::new()
    }

    fn quick_connect_label(&self, target: &str) -> String {
        self.i18n
            .t("command_palette.quick_connect")
            .replace("{{target}}", target)
    }

    pub(super) fn render_command_palette(&self, cx: &mut Context<Self>) -> AnyElement {
        let ranked_items = self.ranked_command_palette_items();
        let (mode, _) = parse_command_palette_mode(&self.command_palette.raw_query);
        let query_text = if self.command_palette.raw_query.is_empty() {
            self.i18n.t("command_palette.placeholder")
        } else {
            self.command_palette.raw_query.clone()
        };
        let mut rows = Vec::new();
        let mut previous_section = None;
        for (index, ranked) in ranked_items.iter().enumerate() {
            if previous_section != Some(ranked.item.section) {
                previous_section = Some(ranked.item.section);
                rows.push(self.render_command_palette_section_heading(ranked.item.section));
            }
            rows.push(self.render_command_palette_row(ranked, index, cx));
        }
        if ranked_items.is_empty() {
            rows.push(
                div()
                    .px(px(16.0))
                    .py(px(20.0))
                    .text_size(px(14.0))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("command_palette.no_results"))
                    .into_any_element(),
            );
        }

        let mut panel = dialog_content(&self.tokens)
            .w(px(COMMAND_PALETTE_WIDTH))
            .rounded(px(self.tokens.radii.lg))
            .shadow_xl()
            .child(
                div()
                    .h(px(48.0))
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .child(Self::render_lucide_icon(
                        LucideIcon::Search,
                        16.0,
                        rgb(self.tokens.ui.text_muted),
                    ))
                    .when(mode != PaletteMode::All, |row| {
                        row.child(self.render_command_palette_mode_badge(mode))
                    })
                    .child(
                        div()
                            .ml(px(8.0))
                            .flex_1()
                            .min_w_0()
                            .text_size(px(14.0))
                            .text_color(if self.command_palette.raw_query.is_empty() {
                                rgb(self.tokens.ui.text_muted)
                            } else {
                                rgb(self.tokens.ui.text)
                            })
                            .child(query_text),
                    ),
            )
            .child(
                div()
                    .max_h(px(COMMAND_PALETTE_LIST_MAX_HEIGHT))
                    .overflow_y_scrollbar()
                    .py(px(6.0))
                    .children(rows),
            );
        if let Some(error) = self.command_palette.error.as_ref() {
            panel = panel.child(
                div()
                    .border_t_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .px(px(12.0))
                    .py(px(8.0))
                    .text_size(px(12.0))
                    .text_color(rgb(self.tokens.ui.error))
                    .child(error.clone()),
            );
        }
        panel = panel.child(
            div()
                .border_t_1()
                .border_color(rgb(self.tokens.ui.border))
                .px(px(12.0))
                .py(px(6.0))
                .flex()
                .items_center()
                .gap(px(12.0))
                .text_size(px(11.0))
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(self.i18n.t("command_palette.footer_hint")),
        );
        let palette_top = self
            .ai_overlay_window_size
            .map(|(_, height)| height * COMMAND_PALETTE_TOP_RATIO)
            .unwrap_or(COMMAND_PALETTE_FALLBACK_TOP);

        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .items_start()
            .justify_center()
            .bg(rgba((0x000000 << 8) | COMMAND_PALETTE_BACKDROP_ALPHA))
            .occlude()
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(div().mt(px(palette_top)).child(panel))
            .into_any_element()
    }

    fn render_command_palette_mode_badge(&self, mode: PaletteMode) -> AnyElement {
        div()
            .ml(px(8.0))
            .mr(px(2.0))
            .rounded(px(self.tokens.radii.xs))
            .bg(rgba((self.tokens.ui.accent << 8) | 0x33))
            .px(px(6.0))
            .py(px(2.0))
            .text_size(px(12.0))
            .font_family("monospace")
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(rgb(self.tokens.ui.accent))
            .child(match mode {
                PaletteMode::All => "",
                PaletteMode::Commands => ">",
                PaletteMode::Sessions => "@",
                PaletteMode::Connections => "#",
            })
            .into_any_element()
    }

    fn render_command_palette_section_heading(&self, section: PaletteSection) -> AnyElement {
        div()
            .px(px(12.0))
            .py(px(6.0))
            .text_size(px(12.0))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(self.i18n.t(section_label_key(section)))
            .into_any_element()
    }

    fn render_command_palette_row(
        &self,
        ranked: &RankedItem,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = index == self.command_palette.selected_index;
        let item = ranked.item.clone();
        let mut label_column = div().flex().flex_col().min_w_0().gap(px(2.0)).child(
            self.render_highlighted_palette_text(&ranked.item.label, &ranked.highlights, selected),
        );
        if let Some(detail) = ranked.item.detail.as_ref() {
            label_column = label_column.child(
                div()
                    .text_size(px(12.0))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .truncate()
                    .child(detail.clone()),
            );
        }
        div()
            .id(("command-palette-row", index))
            .min_h(px(COMMAND_PALETTE_ROW_HEIGHT))
            .px(px(12.0))
            .py(px(6.0))
            .mx(px(6.0))
            .rounded(px(self.tokens.radii.sm))
            .flex()
            .items_center()
            .gap(px(10.0))
            .bg(if selected {
                rgba((self.tokens.ui.accent << 8) | COMMAND_PALETTE_SELECTED_ALPHA)
            } else {
                rgba(0x00000000)
            })
            .text_color(if selected {
                rgb(self.tokens.ui.accent)
            } else {
                rgb(self.tokens.ui.text)
            })
            .cursor(CursorStyle::PointingHand)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    this.execute_command_palette_item(item.clone(), window, cx);
                    cx.stop_propagation();
                }),
            )
            .child(Self::render_lucide_icon(
                ranked.item.icon,
                16.0,
                if selected {
                    rgb(self.tokens.ui.accent)
                } else {
                    rgb(self.tokens.ui.text_muted)
                },
            ))
            .child(label_column)
            .when(ranked.item.disabled, |row| {
                row.child(
                    div()
                        .ml_auto()
                        .rounded(px(self.tokens.radii.xs))
                        .bg(rgb(self.tokens.ui.bg_panel))
                        .px(px(6.0))
                        .py(px(3.0))
                        .text_size(px(11.0))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(self.i18n.t("common.disabled")),
                )
            })
            .when_some(ranked.item.shortcut.as_ref(), |row, shortcut| {
                row.child(
                    div()
                        .ml_auto()
                        .rounded(px(self.tokens.radii.sm))
                        .border_1()
                        .border_color(rgb(self.tokens.ui.border))
                        .px(px(8.0))
                        .py(px(4.0))
                        .text_size(px(12.0))
                        .font_family("monospace")
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(shortcut.clone()),
                )
            })
            .into_any_element()
    }

    fn render_highlighted_palette_text(
        &self,
        text: &str,
        highlights: &[usize],
        selected: bool,
    ) -> AnyElement {
        let mut label = div().flex().items_center().min_w_0().text_size(px(14.0));
        let highlight_set = highlights.iter().copied().collect::<HashSet<_>>();
        for (index, ch) in text.chars().enumerate() {
            let highlighted = highlight_set.contains(&index);
            label = label.child(
                div()
                    .text_color(if highlighted || selected {
                        rgb(self.tokens.ui.accent)
                    } else {
                        rgb(self.tokens.ui.text)
                    })
                    .when(highlighted, |part| {
                        part.font_weight(gpui::FontWeight::SEMIBOLD)
                    })
                    .child(ch.to_string()),
            );
        }
        label.into_any_element()
    }

    pub(super) fn render_shortcuts_modal(&self, _cx: &mut Context<Self>) -> AnyElement {
        let rows = self.filtered_shortcut_rows();
        let query_text = if self.shortcuts_modal.query.is_empty() {
            self.i18n.t("settings_view.keybindings.search_placeholder")
        } else {
            self.shortcuts_modal.query.clone()
        };
        dialog_backdrop()
            .child(
                dialog_content(&self.tokens)
                    .w(px(760.0))
                    .max_h(px(640.0))
                    .child(
                        div()
                            .px(px(24.0))
                            .py(px(18.0))
                            .border_b_1()
                            .border_color(rgb(self.tokens.ui.border))
                            .child(
                                div()
                                    .text_size(px(18.0))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(self.tokens.ui.text_heading))
                                    .child(self.i18n.t("layout.empty.keyboard_shortcuts")),
                            )
                            .child(
                                div()
                                    .mt(px(14.0))
                                    .h(px(44.0))
                                    .rounded(px(self.tokens.radii.sm))
                                    .border_1()
                                    .border_color(rgb(self.tokens.ui.border))
                                    .bg(rgb(self.tokens.ui.bg))
                                    .px(px(14.0))
                                    .flex()
                                    .items_center()
                                    .text_size(px(15.0))
                                    .text_color(if self.shortcuts_modal.query.is_empty() {
                                        rgb(self.tokens.ui.text_muted)
                                    } else {
                                        rgb(self.tokens.ui.text)
                                    })
                                    .child(query_text),
                            ),
                    )
                    .child(
                        div()
                            .max_h(px(500.0))
                            .overflow_y_scrollbar()
                            .p(px(16.0))
                            .children(rows),
                    ),
            )
            .into_any_element()
    }

    fn filtered_shortcut_rows(&self) -> Vec<AnyElement> {
        let query = self.shortcuts_modal.query.trim().to_lowercase();
        let side = crate::keybindings::KeybindingSide::current();
        let overrides = &self.settings_store.settings().keybindings.overrides;
        let mut rows = Vec::new();
        for definition in crate::keybindings::ACTION_DEFINITIONS.iter() {
            let label = self.i18n.t(&definition.label_key());
            let scope = self.i18n.t(definition.scope.label_key());
            let shortcut = crate::keybindings::format_combo(&crate::keybindings::effective_combo(
                definition, overrides, side,
            ));
            if !query.is_empty()
                && !label.to_lowercase().contains(&query)
                && !shortcut.to_lowercase().contains(&query)
                && !scope.to_lowercase().contains(&query)
            {
                continue;
            }
            rows.push((scope, label, shortcut));
        }
        for (category_key, shortcut_rows) in shortcut_reference_rows() {
            let scope = self.i18n.t(category_key);
            for (label_key, mac, other) in shortcut_rows {
                let label = self.i18n.t(label_key);
                let shortcut = if cfg!(target_os = "macos") {
                    mac
                } else {
                    other
                }
                .to_string();
                if !query.is_empty()
                    && !label.to_lowercase().contains(&query)
                    && !shortcut.to_lowercase().contains(&query)
                    && !scope.to_lowercase().contains(&query)
                {
                    continue;
                }
                rows.push((scope.clone(), label, shortcut));
            }
        }
        let row_count = rows.len();
        rows.into_iter()
            .enumerate()
            .map(|(index, (scope, label, shortcut))| {
                self.render_shortcut_row(scope, label, shortcut, index + 1 < row_count)
            })
            .collect()
    }

    fn render_shortcut_row(
        &self,
        scope: String,
        label: String,
        shortcut: String,
        show_separator: bool,
    ) -> AnyElement {
        div()
            .min_h(px(38.0))
            .px(px(10.0))
            .flex()
            .items_center()
            .justify_between()
            .when(show_separator, |row| {
                row.border_b_1()
                    .border_color(rgba((self.tokens.ui.border << 8) | 0x66))
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .w(px(120.0))
                            .text_size(px(12.0))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(scope),
                    )
                    .child(
                        div()
                            .text_size(px(14.0))
                            .text_color(rgb(self.tokens.ui.text))
                            .child(label),
                    ),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(shortcut),
            )
            .into_any_element()
    }
}

pub(super) fn parse_command_palette_mode(raw_query: &str) -> (PaletteMode, String) {
    let trimmed = raw_query.trim_start();
    if let Some(rest) = trimmed.strip_prefix('>') {
        (PaletteMode::Commands, rest.trim_start().to_string())
    } else if let Some(rest) = trimmed.strip_prefix('@') {
        (PaletteMode::Sessions, rest.trim_start().to_string())
    } else if let Some(rest) = trimmed.strip_prefix('#') {
        (PaletteMode::Connections, rest.trim_start().to_string())
    } else {
        (PaletteMode::All, trimmed.to_string())
    }
}

fn rank_palette_section(items: Vec<PaletteItem>, query: &str) -> Vec<RankedItem> {
    let mut ranked = items
        .into_iter()
        .filter_map(|item| rank_palette_item(item, query))
        .collect::<Vec<_>>();
    if !query.is_empty() {
        ranked.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    ranked
}

fn rank_palette_item(item: PaletteItem, query: &str) -> Option<RankedItem> {
    if query.is_empty() {
        return Some(RankedItem {
            item,
            score: 1.0,
            highlights: Vec::new(),
        });
    }
    let haystack = item.value.to_lowercase();
    let label = item.label.to_lowercase();
    let needle = query.to_lowercase();
    if haystack.contains(&needle) {
        let highlights = substring_highlights(&label, &needle).unwrap_or_default();
        return Some(RankedItem {
            item,
            score: 1.0,
            highlights,
        });
    }
    subsequence_highlights(&label, &needle).map(|highlights| RankedItem {
        item,
        score: 0.5,
        highlights,
    })
}

fn substring_highlights(label: &str, needle: &str) -> Option<Vec<usize>> {
    let start_byte = label.find(needle)?;
    let start = label[..start_byte].chars().count();
    let len = needle.chars().count();
    Some((start..start + len).collect())
}

fn subsequence_highlights(label: &str, needle: &str) -> Option<Vec<usize>> {
    let mut highlights = Vec::new();
    let mut needle_chars = needle.chars();
    let mut current = needle_chars.next()?;
    for (index, ch) in label.chars().enumerate() {
        if ch == current {
            highlights.push(index);
            if let Some(next) = needle_chars.next() {
                current = next;
            } else {
                return Some(highlights);
            }
        }
    }
    None
}

fn parse_user_host_port(query: &str) -> Option<(String, String, u16)> {
    let at = query.find(QUICK_CONNECT_REQUIRES_AT)?;
    let username = query[..at].trim();
    let rest = query[at + 1..].trim();
    if username.is_empty() || rest.is_empty() {
        return None;
    }
    let (host, port) = if let Some((host, port)) = rest.rsplit_once(':') {
        let port = port.parse::<u16>().ok()?;
        (host, port)
    } else {
        (rest, 22)
    };
    if host.is_empty() {
        return None;
    }
    Some((username.to_string(), host.to_string(), port))
}

fn section_label_key(section: PaletteSection) -> &'static str {
    match section {
        PaletteSection::QuickConnect => "command_palette.sections.quick_connect",
        PaletteSection::Recent => "command_palette.sections.recent",
        PaletteSection::Commands => "command_palette.sections.commands",
        PaletteSection::Sessions => "command_palette.sections.sessions",
        PaletteSection::Connections => "command_palette.sections.connections",
        PaletteSection::Plugins => "command_palette.sections.plugins",
        PaletteSection::Help => "command_palette.sections.help",
    }
}

fn tab_kind_icon(kind: &TabKind) -> LucideIcon {
    match kind {
        TabKind::LocalTerminal | TabKind::SshTerminal => LucideIcon::Terminal,
        TabKind::FileManager => LucideIcon::FolderOpen,
        TabKind::Launcher => LucideIcon::Terminal,
        TabKind::Graphics => LucideIcon::AppWindow,
        TabKind::ConnectionPool => LucideIcon::Terminal,
        TabKind::ConnectionMonitor => LucideIcon::Activity,
        TabKind::Topology => LucideIcon::Network,
        TabKind::NotificationCenter => LucideIcon::Bell,
        TabKind::Forwards => LucideIcon::ArrowLeftRight,
        TabKind::Sftp => LucideIcon::HardDrive,
        TabKind::Ide => LucideIcon::Code2,
        TabKind::PluginManager => LucideIcon::Puzzle,
        TabKind::Settings => LucideIcon::Settings,
        TabKind::SessionManager => LucideIcon::LayoutList,
    }
}

fn keybinding_command(id: &'static str, action_id: &'static str, icon: LucideIcon) -> CommandSpec {
    CommandSpec {
        id,
        label_key: Cow::Owned(format!("settings_view.keybindings.actions.{action_id}")),
        icon,
        shortcut_action: Some(action_id),
        action: PaletteAction::Keybinding(action_id),
    }
}

fn command_palette_specs() -> Vec<CommandSpec> {
    vec![
        keybinding_command("cmd:new_terminal", "app.newTerminal", LucideIcon::Terminal),
        keybinding_command("cmd:new_connection", "app.newConnection", LucideIcon::Plus),
        keybinding_command("cmd:settings", "app.settings", LucideIcon::Settings),
        keybinding_command(
            "cmd:toggle_sidebar",
            "app.toggleSidebar",
            LucideIcon::PanelLeft,
        ),
        keybinding_command("cmd:zen_mode", "app.zenMode", LucideIcon::AppWindow),
        keybinding_command(
            "cmd:toggle_panel",
            "palette.eventLog",
            LucideIcon::LayoutList,
        ),
        keybinding_command(
            "cmd:toggle_ai_sidebar",
            "palette.aiSidebar",
            LucideIcon::PanelLeft,
        ),
        keybinding_command("cmd:close_tab", "app.closeTab", LucideIcon::X),
        keybinding_command(
            "cmd:split_horizontal",
            "split.horizontal",
            LucideIcon::SplitSquareHorizontal,
        ),
        keybinding_command(
            "cmd:split_vertical",
            "split.vertical",
            LucideIcon::SplitSquareVertical,
        ),
        keybinding_command(
            "cmd:broadcast_toggle",
            "palette.broadcast",
            LucideIcon::Radio,
        ),
        keybinding_command("cmd:next_tab", "app.nextTab", LucideIcon::ChevronRight),
        keybinding_command("cmd:prev_tab", "app.prevTab", LucideIcon::ChevronLeft),
        keybinding_command(
            "cmd:close_other_tabs",
            "app.closeOtherTabs",
            LucideIcon::Layers,
        ),
        CommandSpec {
            id: "cmd:close_all_tabs",
            label_key: "command_palette.commands.close_all_tabs".into(),
            icon: LucideIcon::Layers,
            shortcut_action: None,
            action: PaletteAction::CloseAllTabs,
        },
        keybinding_command("cmd:go_back", "app.navBack", LucideIcon::ArrowDownRight),
        keybinding_command("cmd:go_forward", "app.navForward", LucideIcon::ArrowRight),
        CommandSpec {
            id: "cmd:open_connection_manager",
            label_key: "command_palette.commands.open_connection_manager".into(),
            icon: LucideIcon::FolderOpen,
            shortcut_action: None,
            action: PaletteAction::OpenSessionManager,
        },
        CommandSpec {
            id: "cmd:theme_next",
            label_key: "command_palette.commands.theme_next".into(),
            icon: LucideIcon::Sparkles,
            shortcut_action: None,
            action: PaletteAction::ThemeNext(true),
        },
        CommandSpec {
            id: "cmd:theme_prev",
            label_key: "command_palette.commands.theme_prev".into(),
            icon: LucideIcon::Sparkles,
            shortcut_action: None,
            action: PaletteAction::ThemeNext(false),
        },
        keybinding_command("cmd:font_increase", "app.fontIncrease", LucideIcon::Plus),
        keybinding_command(
            "cmd:font_decrease",
            "app.fontDecrease",
            LucideIcon::ArrowDown,
        ),
        keybinding_command("cmd:font_reset", "app.fontReset", LucideIcon::RotateCcw),
        CommandSpec {
            id: "cmd:cursor_block",
            label_key: "command_palette.commands.cursor_block".into(),
            icon: LucideIcon::Square,
            shortcut_action: None,
            action: PaletteAction::CursorStyle(SettingsCursorStyle::Block),
        },
        CommandSpec {
            id: "cmd:cursor_bar",
            label_key: "command_palette.commands.cursor_bar".into(),
            icon: LucideIcon::Terminal,
            shortcut_action: None,
            action: PaletteAction::CursorStyle(SettingsCursorStyle::Bar),
        },
        CommandSpec {
            id: "cmd:cursor_underline",
            label_key: "command_palette.commands.cursor_underline".into(),
            icon: LucideIcon::ArrowDown,
            shortcut_action: None,
            action: PaletteAction::CursorStyle(SettingsCursorStyle::Underline),
        },
        CommandSpec {
            id: "cmd:sidebar_sessions",
            label_key: "command_palette.commands.sidebar_sessions".into(),
            icon: LucideIcon::ListTree,
            shortcut_action: None,
            action: PaletteAction::Sidebar(SidebarSection::Sessions),
        },
        CommandSpec {
            id: "cmd:sidebar_saved",
            label_key: "command_palette.commands.sidebar_saved".into(),
            icon: LucideIcon::Server,
            shortcut_action: None,
            action: PaletteAction::Sidebar(SidebarSection::Connections),
        },
        CommandSpec {
            id: "cmd:sidebar_sftp",
            label_key: "command_palette.commands.sidebar_sftp".into(),
            icon: LucideIcon::HardDrive,
            shortcut_action: None,
            action: PaletteAction::Sidebar(SidebarSection::Terminal),
        },
        CommandSpec {
            id: "cmd:sidebar_forwards",
            label_key: "command_palette.commands.sidebar_forwards".into(),
            icon: LucideIcon::ArrowLeftRight,
            shortcut_action: None,
            action: PaletteAction::Sidebar(SidebarSection::Activity),
        },
        CommandSpec {
            id: "cmd:sidebar_connections",
            label_key: "command_palette.commands.sidebar_connections".into(),
            icon: LucideIcon::Network,
            shortcut_action: None,
            action: PaletteAction::Sidebar(SidebarSection::Network),
        },
        CommandSpec {
            id: "cmd:sidebar_ai",
            label_key: "command_palette.commands.sidebar_ai".into(),
            icon: LucideIcon::Bot,
            shortcut_action: None,
            action: PaletteAction::Keybinding("palette.aiSidebar"),
        },
        CommandSpec {
            id: "cmd:open_connection_pool",
            label_key: "command_palette.commands.open_connection_pool".into(),
            icon: LucideIcon::Activity,
            shortcut_action: None,
            action: PaletteAction::OpenConnectionPool,
        },
        CommandSpec {
            id: "cmd:open_connection_monitor",
            label_key: "command_palette.commands.open_connection_monitor".into(),
            icon: LucideIcon::Gauge,
            shortcut_action: None,
            action: PaletteAction::OpenConnectionMonitor,
        },
        CommandSpec {
            id: "cmd:shell_launcher",
            label_key: "settings_view.keybindings.actions.app.shellLauncher".into(),
            icon: LucideIcon::Terminal,
            shortcut_action: Some("app.shellLauncher"),
            action: PaletteAction::Keybinding("app.shellLauncher"),
        },
        CommandSpec {
            id: "cmd:toggle_fps",
            label_key: "command_palette.commands.toggle_fps".into(),
            icon: LucideIcon::Gauge,
            shortcut_action: None,
            action: PaletteAction::ToggleFps,
        },
        keybinding_command(
            "cmd:close_pane",
            "split.closePane",
            LucideIcon::PanelLeftClose,
        ),
        keybinding_command(
            "cmd:focus_next_pane",
            "split.navRight",
            LucideIcon::CornerDownLeft,
        ),
        CommandSpec {
            id: "cmd:open_plugin_manager",
            label_key: "command_palette.commands.open_plugin_manager".into(),
            icon: LucideIcon::Puzzle,
            shortcut_action: None,
            action: PaletteAction::OpenPluginManager,
        },
        CommandSpec {
            id: "cmd:open_topology",
            label_key: "command_palette.commands.open_topology".into(),
            icon: LucideIcon::Network,
            shortcut_action: None,
            action: PaletteAction::OpenTopology,
        },
        CommandSpec {
            id: "cmd:open_notification_center",
            label_key: "command_palette.commands.open_notification_center".into(),
            icon: LucideIcon::Bell,
            shortcut_action: None,
            action: PaletteAction::OpenNotificationCenter,
        },
    ]
}

fn help_palette_specs() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: "cmd:show_shortcuts",
        label_key: "settings_view.keybindings.actions.app.showShortcuts".into(),
        icon: LucideIcon::Keyboard,
        shortcut_action: Some("app.showShortcuts"),
        action: PaletteAction::Keybinding("app.showShortcuts"),
    }]
}

fn shortcut_reference_rows() -> Vec<(
    &'static str,
    Vec<(&'static str, &'static str, &'static str)>,
)> {
    vec![
        (
            "settings_view.help.category_file_manager",
            vec![
                ("settings_view.help.shortcut_select_all", "⌘A", "Ctrl+A"),
                ("settings_view.help.shortcut_copy", "⌘C", "Ctrl+C"),
                ("settings_view.help.shortcut_cut", "⌘X", "Ctrl+X"),
                ("settings_view.help.shortcut_paste", "⌘V", "Ctrl+V"),
                ("settings_view.help.shortcut_rename", "F2", "F2"),
                ("settings_view.help.shortcut_delete", "Delete", "Delete"),
                ("settings_view.help.shortcut_quick_look", "Space", "Space"),
                ("settings_view.help.shortcut_open", "Enter", "Enter"),
            ],
        ),
        (
            "settings_view.help.category_sftp",
            vec![
                ("settings_view.help.shortcut_select_all", "⌘A", "Ctrl+A"),
                ("settings_view.help.shortcut_quick_look", "Space", "Space"),
                (
                    "settings_view.help.shortcut_sftp_enter_dir",
                    "Enter",
                    "Enter",
                ),
                ("settings_view.help.shortcut_sftp_upload", "→", "→"),
                ("settings_view.help.shortcut_sftp_download", "←", "←"),
                ("settings_view.help.shortcut_rename", "F2", "F2"),
                ("settings_view.help.shortcut_delete", "Delete", "Delete"),
            ],
        ),
        (
            "settings_view.help.category_editor",
            vec![
                ("settings_view.help.shortcut_save", "⌘S", "Ctrl+S"),
                ("settings_view.help.shortcut_find", "⌘F", "Ctrl+F"),
                ("settings_view.help.shortcut_copy", "⌘C", "Ctrl+C"),
                ("settings_view.help.shortcut_paste", "⌘V", "Ctrl+V"),
                ("settings_view.help.shortcut_close", "Esc", "Esc"),
            ],
        ),
    ]
}
