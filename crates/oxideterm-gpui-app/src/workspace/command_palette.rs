use super::*;
use oxideterm_connections::{
    list_ssh_config_hosts, resolve_ssh_config_alias, saved_connection_from_ssh_host,
};
use oxideterm_gpui_settings_view::{OXIDE_THEME_IDS, built_in_theme_exists, is_oxide_theme};
use oxideterm_gpui_ui::{
    modal::{
        dialog_content, dismissible_command_palette_backdrop, dismissible_dialog_backdrop,
        overlay_content_boundary,
    },
    text_input::{text_input_anchor_probe, text_input_value_segments},
};
use oxideterm_theme::BUILT_IN_THEMES;
use std::borrow::Cow;

const COMMAND_PALETTE_WIDTH: f32 = 560.0; // Tauri DialogContent max-w-[560px].
const COMMAND_PALETTE_FALLBACK_TOP: f32 = 96.0;
const COMMAND_PALETTE_TOP_RATIO: f32 = 0.15; // Tauri DialogContent top-[15%] translate-y-0.
const COMMAND_PALETTE_LIST_MAX_HEIGHT: f32 = 400.0; // Tauri CommandList max-h-[min(50vh,400px)] cap.
const COMMAND_PALETTE_INPUT_HEIGHT: f32 = 40.0; // Tauri CommandInput h-10.
const COMMAND_PALETTE_VIRTUAL_ROW_HEIGHT: f32 = 32.0; // Tauri CommandItem py-1.5 + text-sm line height.
const COMMAND_PALETTE_VIRTUAL_OVERSCAN: usize = 8; // Browser CommandList keeps a small DOM buffer around the viewport.
const SHORTCUTS_MODAL_LIST_MAX_HEIGHT: f32 = 420.0;
const SHORTCUTS_MODAL_VIRTUAL_ROW_HEIGHT: f32 = 32.0;
const SHORTCUTS_MODAL_VIRTUAL_OVERSCAN: usize = 8; // Tauri shortcuts modal uses the same command-row rhythm.
const COMMAND_PALETTE_ICON_SLOT: f32 = 16.0; // Tauri CommandInput/CommandItem h-4 w-4 icons.
const COMMAND_PALETTE_ITEM_GAP: f32 = 10.0; // Tauri CommandItem gap-2.5.
const COMMAND_PALETTE_SELECTED_ALPHA: u32 = 0x26; // Tauri accent/15.
const COMMAND_PALETTE_MODE_BADGE_ALPHA: u32 = 0x33; // Tauri accent/20.
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
    OpenSavedConnections,
    OpenSessionManager,
    OpenConnectionPool,
    OpenTopology,
    OpenPluginManager,
    OpenCloudSync,
    CloseAllTabs,
    DisconnectAll,
    ReconnectAll,
    CancelReconnect,
    HealthCheck,
    ResetPanes,
    DetachTerminal,
    CleanupDead,
    ResetSettings,
    ThemeNext(bool),
    CursorStyle(SettingsCursorStyle),
    ToggleFps,
    RuntimePluginCommand {
        plugin_id: String,
        command: String,
    },
    PluginCommandPending,
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

#[derive(Clone)]
enum CommandPaletteVirtualRow {
    Heading(PaletteSection),
    Item {
        ranked: RankedItem,
        item_index: usize,
    },
    Empty,
}

#[derive(Clone)]
enum ShortcutsModalVirtualRow {
    Heading(String),
    Row {
        row: ShortcutModalRow,
        show_separator: bool,
    },
    Empty,
}

impl WorkspaceApp {
    pub(super) fn open_command_palette(&mut self, cx: &mut Context<Self>) {
        self.command_palette.open = true;
        self.command_palette.raw_query.clear();
        self.command_palette.mode = PaletteMode::All;
        self.command_palette.selected_index = 0;
        self.command_palette.error = None;
        self.ime_marked_text = None;
        self.command_palette.scroll_handle = UniformListScrollHandle::new();
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
        self.shortcuts_modal.scroll_handle = UniformListScrollHandle::new();
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
                    self.scroll_selected_command_palette_item_into_view();
                    cx.notify();
                }
            }
            "arrowup" | "up" => {
                self.command_palette.selected_index =
                    self.command_palette.selected_index.saturating_sub(1);
                self.scroll_selected_command_palette_item_into_view();
                cx.notify();
            }
            "pagedown" => {
                let count = self.filtered_command_palette_items().len();
                if count > 0 {
                    self.command_palette.selected_index =
                        (self.command_palette.selected_index + 8).min(count - 1);
                    self.scroll_selected_command_palette_item_into_view();
                }
                cx.notify();
            }
            "pageup" => {
                self.command_palette.selected_index =
                    self.command_palette.selected_index.saturating_sub(8);
                self.scroll_selected_command_palette_item_into_view();
                cx.notify();
            }
            "home" => {
                self.command_palette.selected_index = 0;
                self.scroll_selected_command_palette_item_into_view();
                cx.notify();
            }
            "end" => {
                let count = self.filtered_command_palette_items().len();
                if count > 0 {
                    self.command_palette.selected_index = count - 1;
                    self.scroll_selected_command_palette_item_into_view();
                }
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
        self.command_palette.scroll_handle = UniformListScrollHandle::new();
        cx.notify();
    }

    fn scroll_selected_command_palette_item_into_view(&self) {
        let ranked_items = self.ranked_command_palette_items();
        if let Some(child_index) =
            command_palette_scroll_child_index(&ranked_items, self.command_palette.selected_index)
        {
            // Tauri cmdk reveals the selected item automatically; GPUI scroll
            // children include section headings, so we map the selected item
            // index to the actual scroll child index before requesting reveal.
            scroll_tauri_virtual_list_to_index(
                &self.command_palette.scroll_handle,
                child_index,
                TauriVirtualListSpec::new(
                    px(COMMAND_PALETTE_VIRTUAL_ROW_HEIGHT),
                    COMMAND_PALETTE_VIRTUAL_OVERSCAN,
                ),
                TauriVirtualScrollAlign::Center,
            );
        }
    }

    fn render_overlay_query_input(
        &self,
        target: WorkspaceImeTarget,
        value: &str,
        placeholder: String,
        text_size: f32,
        line_height: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let visually_empty = value.is_empty();
        let display = if visually_empty {
            placeholder
        } else {
            value.to_string()
        };
        let selected_range = self.ime_selected_range_for_target(target);
        let selection_range = selected_range
            .as_ref()
            .filter(|range| range.start < range.end)
            .cloned();
        let caret_offset = selected_range
            .as_ref()
            .filter(|range| range.start == range.end)
            .map(|range| range.start);
        let marked_text = self.marked_text_for_target(target).unwrap_or_default();
        let workspace = cx.entity();

        text_input_anchor_probe(
            target.anchor_id(),
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .text_size(px(text_size))
                .line_height(px(line_height))
                .text_color(if visually_empty {
                    rgb(self.tokens.ui.text_muted)
                } else {
                    rgb(self.tokens.ui.text)
                })
                .cursor(gpui::CursorStyle::IBeam)
                .overflow_hidden()
                .child(text_input_value_segments(
                    &self.tokens,
                    &display,
                    visually_empty,
                    selection_range,
                    caret_offset,
                    self.new_connection_caret_visible,
                ))
                .when(!marked_text.is_empty(), |input| {
                    input.child(
                        div()
                            .underline()
                            .text_color(rgb(self.tokens.ui.text))
                            .child(marked_text.to_string()),
                    )
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        window.focus(&this.focus_handle);
                        this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                    this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                })),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
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
                self.shortcuts_modal.scroll_handle = UniformListScrollHandle::new();
                cx.notify();
            }
            _ => {
                if let Some(text) = event.keystroke.key_char.as_deref()
                    && !event.keystroke.modifiers.platform
                    && !event.keystroke.modifiers.control
                    && !text.chars().any(char::is_control)
                {
                    self.shortcuts_modal.query.push_str(text);
                    self.shortcuts_modal.scroll_handle = UniformListScrollHandle::new();
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
            PaletteAction::OpenSavedConnections => self.open_session_manager_tab(window, cx),
            PaletteAction::OpenSessionManager => self.open_session_manager_tab(window, cx),
            PaletteAction::OpenConnectionPool => self.open_connection_pool_tab(window, cx),
            PaletteAction::OpenTopology => self.open_topology_tab(window, cx),
            PaletteAction::OpenPluginManager => self.open_plugin_manager_tab(window, cx),
            PaletteAction::OpenCloudSync => self.open_cloud_sync_tab(window, cx),
            PaletteAction::CloseAllTabs => self.close_all_tabs_from_palette(window, cx),
            PaletteAction::DisconnectAll => self.disconnect_all_ssh_nodes_from_palette(window, cx),
            PaletteAction::ReconnectAll => self.reconnect_all_link_down_nodes_from_palette(cx),
            PaletteAction::CancelReconnect => self.cancel_all_reconnects_from_palette(cx),
            PaletteAction::HealthCheck => self.run_connection_health_check_from_palette(cx),
            PaletteAction::ResetPanes => self.reset_active_tab_to_single_pane(window, cx),
            PaletteAction::DetachTerminal => {
                self.detach_active_local_terminal_from_palette(window, cx);
            }
            PaletteAction::CleanupDead => {
                self.cleanup_dead_local_terminal_sessions_from_palette(cx)
            }
            PaletteAction::ResetSettings => self.open_reset_settings_confirm_from_palette(cx),
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
            PaletteAction::RuntimePluginCommand { plugin_id, command } => {
                self.dispatch_native_plugin_command(plugin_id, command, cx);
            }
            PaletteAction::PluginCommandPending => {
                self.command_palette.error =
                    Some("Plugin command runtime is not available yet.".to_string());
            }
        }
        cx.notify();
    }

    pub(super) fn push_command_palette_toast(
        &self,
        title: String,
        description: Option<String>,
        variant: TerminalNoticeVariant,
    ) {
        let _ = self.terminal_notice_tx.send(TerminalNotice {
            title,
            description,
            status_text: None,
            progress: None,
            variant,
        });
    }

    pub(super) fn i18n_replace(&self, key: &str, replacements: &[(&str, String)]) -> String {
        let mut text = self.i18n.t(key);
        for (name, value) in replacements {
            text = text.replace(&format!("{{{{{name}}}}}"), value);
        }
        text
    }

    pub(super) fn disconnect_all_ssh_nodes_from_palette(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut root_ids = self.node_router.export_tree_snapshot().root_ids;
        if root_ids.is_empty() {
            root_ids = self.ssh_nodes.keys().cloned().collect();
        }
        root_ids.sort_by(|left, right| left.0.cmp(&right.0));
        root_ids.dedup();

        let mut disconnected = 0usize;
        for node_id in root_ids {
            if self.ssh_nodes.contains_key(&node_id) {
                self.disconnect_ssh_node(&node_id, window, cx);
                disconnected += 1;
            }
        }

        let _ = disconnected;
    }

    pub(super) fn cancel_all_reconnects_from_palette(&mut self, cx: &mut Context<Self>) {
        let active_jobs = self
            .reconnect_orchestrator
            .jobs()
            .into_iter()
            .filter(|job| job.ended_at.is_none())
            .map(|job| NodeId::new(job.node_id))
            .collect::<Vec<_>>();
        for node_id in active_jobs {
            self.cancel_reconnect_for_node(&node_id, cx);
        }
    }

    pub(super) fn run_connection_health_check_from_palette(&mut self, cx: &mut Context<Self>) {
        let summaries = self.ssh_registry.list_connection_summaries();
        let total = summaries.len();
        let healthy = summaries
            .iter()
            .filter(|summary| {
                matches!(
                    summary.state,
                    ConnectionPoolEntryState::Active | ConnectionPoolEntryState::Idle
                )
            })
            .count();
        self.connection_monitor.pool_stats = Some(self.ssh_registry.monitor_stats());
        self.connection_monitor.pool_summaries = summaries;
        self.push_command_palette_toast(
            self.i18n_replace(
                "command_palette.health_result",
                &[
                    ("healthy", healthy.to_string()),
                    ("total", total.to_string()),
                ],
            ),
            None,
            TerminalNoticeVariant::Success,
        );
        cx.notify();
    }

    pub(super) fn open_reset_settings_confirm_from_palette(&mut self, cx: &mut Context<Self>) {
        self.settings_page.set_settings_reset_confirm_open(true);
        self.reset_standard_confirm_focus();
        cx.notify();
    }

    pub(super) fn render_settings_reset_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        confirm_dialog_with_focus(
            &self.tokens,
            ConfirmDialogView {
                variant: ConfirmDialogVariant::Danger,
                title: div()
                    .child(self.i18n.t("command_palette.cmd_reset_settings"))
                    .into_any_element(),
                description: Some(
                    div()
                        .child(self.i18n.t("command_palette.confirm_reset_settings"))
                        .into_any_element(),
                ),
                cancel_label: div()
                    .child(self.i18n.t("common.actions.cancel"))
                    .into_any_element(),
                confirm_label: div()
                    .child(self.i18n.t("command_palette.cmd_reset_settings"))
                    .into_any_element(),
            },
            self.standard_confirm_focus(),
            cx.listener(|this, _event, _window, cx| {
                this.settings_page.set_settings_reset_confirm_open(false);
                this.clear_standard_confirm_focus();
                cx.stop_propagation();
                cx.notify();
            }),
            cx.listener(|this, _event, _window, cx| {
                this.settings_page.set_settings_reset_confirm_open(false);
                this.clear_standard_confirm_focus();
                this.edit_settings(|settings| *settings = PersistedSettings::default(), cx);
                cx.stop_propagation();
            }),
        )
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
        self.close_new_connection_select();
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
                    self.close_new_connection_select();
                    self.new_connection_caret_visible = true;
                    self.needs_active_pane_focus = false;
                    window.focus(&self.focus_handle);
                }
                Err(error) => self.command_palette.error = Some(error.to_string()),
            },
            Ok(None) => {
                self.command_palette.error = Some(
                    self.i18n
                        .t("command_palette.quick_connect_alias_not_found")
                        .replace("{{alias}}", &alias),
                );
            }
            Err(error) => {
                let message = self
                    .i18n
                    .t("command_palette.quick_connect_resolve_failed")
                    .replace("{{alias}}", &alias);
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
            let target = format!("{username}@{host}:{port}");
            return Some(PaletteItem {
                id: "quick_connect".to_string(),
                label: self.quick_connect_label(&target),
                section: PaletteSection::QuickConnect,
                icon: LucideIcon::Zap,
                detail: None,
                shortcut: None,
                value: format!("quick_connect {target}"),
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
                detail: None,
                shortcut: None,
                value: format!("quick_connect {alias}"),
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
                    TabKind::Plugin { .. } => self.i18n.t("sidebar.panels.plugins"),
                    TabKind::CloudSync => self.i18n.t("plugin.cloud_sync.panel_title"),
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
        let contributions = self.plugin_registry.contributions();
        let mut items = Vec::new();
        items.extend(contributions.api_commands.iter().map(|command| {
            // Phase 2 mirrors Tauri command registry visibility without
            // executing handlers before the native runtime bridge exists.
            PaletteItem {
                id: format!("plugin-command:{}:{}", command.plugin_id, command.command),
                label: format!("{}: {}", command.plugin_name, command.command),
                section: PaletteSection::Plugins,
                icon: LucideIcon::Puzzle,
                detail: Some("等待 native plugin runtime".to_string()),
                shortcut: None,
                value: format!(
                    "{} {} {}",
                    command.plugin_name, command.plugin_id, command.command
                ),
                action: PaletteAction::PluginCommandPending,
                disabled: true,
            }
        }));
        items.extend(contributions.runtime_commands.iter().map(|command| {
            // Tauri registerCommand installs a command palette entry backed by
            // a plugin handler. Native dispatches the same command id through
            // the process runtime RPC boundary instead of running JS handlers.
            PaletteItem {
                id: format!(
                    "plugin-runtime-command:{}:{}",
                    command.plugin_id, command.registration_id
                ),
                label: format!("{}: {}", command.plugin_name, command.label),
                section: PaletteSection::Plugins,
                icon: LucideIcon::Puzzle,
                detail: Some("插件命令".to_string()),
                shortcut: command.shortcut.clone(),
                value: format!(
                    "{} {} {} {}",
                    command.plugin_name, command.plugin_id, command.command, command.label
                ),
                action: PaletteAction::RuntimePluginCommand {
                    plugin_id: command.plugin_id.clone(),
                    command: command.command.clone(),
                },
                disabled: false,
            }
        }));
        items.extend(contributions.runtime_keybindings.iter().map(|keybinding| {
            // Tauri registerKeybinding stores a key combo plus handler. Native
            // keeps the keybinding as host-owned metadata and dispatches the
            // associated command through the same runtime RPC path as commands.
            PaletteItem {
                id: format!(
                    "plugin-runtime-keybinding:{}:{}",
                    keybinding.plugin_id, keybinding.registration_id
                ),
                label: format!("{}: {}", keybinding.plugin_name, keybinding.label),
                section: PaletteSection::Plugins,
                icon: LucideIcon::Keyboard,
                detail: Some("插件快捷键".to_string()),
                shortcut: Some(keybinding.keybinding.clone()),
                value: format!(
                    "{} {} {} {}",
                    keybinding.plugin_name,
                    keybinding.plugin_id,
                    keybinding.command,
                    keybinding.keybinding
                ),
                action: PaletteAction::RuntimePluginCommand {
                    plugin_id: keybinding.plugin_id.clone(),
                    command: keybinding.command.clone(),
                },
                disabled: false,
            }
        }));
        items.extend(
            contributions
                .terminal_shortcuts
                .iter()
                .map(|shortcut| PaletteItem {
                    id: format!(
                        "plugin-shortcut:{}:{}",
                        shortcut.plugin_id, shortcut.definition.command
                    ),
                    label: format!("{}: {}", shortcut.plugin_name, shortcut.definition.command),
                    section: PaletteSection::Plugins,
                    icon: LucideIcon::Keyboard,
                    detail: Some("插件终端快捷键等待 runtime".to_string()),
                    shortcut: Some(shortcut.definition.key.clone()),
                    value: format!(
                        "{} {} {} {}",
                        shortcut.plugin_name,
                        shortcut.plugin_id,
                        shortcut.definition.command,
                        shortcut.definition.key
                    ),
                    action: PaletteAction::PluginCommandPending,
                    disabled: true,
                }),
        );
        items
    }

    fn quick_connect_label(&self, target: &str) -> String {
        format!("{}: {target}", self.i18n.t("command_palette.quick_connect"))
    }

    pub(super) fn render_command_palette(&self, cx: &mut Context<Self>) -> AnyElement {
        let ranked_items = self.ranked_command_palette_items();
        let (mode, _) = parse_command_palette_mode(&self.command_palette.raw_query);
        let query_text = if self.command_palette.raw_query.is_empty() {
            self.i18n.t(command_palette_placeholder_key(mode))
        } else {
            self.command_palette.raw_query.clone()
        };
        let rows = Arc::new(command_palette_virtual_rows(ranked_items));
        let row_count = rows.len();
        let rows_height = (row_count as f32 * COMMAND_PALETTE_VIRTUAL_ROW_HEIGHT)
            .min(COMMAND_PALETTE_LIST_MAX_HEIGHT);
        let virtual_rows = rows.clone();
        let entity = cx.entity();

        let panel = dialog_content(&self.tokens)
            .w(px(COMMAND_PALETTE_WIDTH))
            .rounded(px(self.tokens.radii.lg))
            .shadow_xl()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .rounded(px(self.tokens.radii.md))
                    .bg(rgb(self.tokens.ui.bg))
                    .child(
                        div()
                            .h(px(COMMAND_PALETTE_INPUT_HEIGHT))
                            .px(px(12.0))
                            .flex()
                            .items_center()
                            .border_b_1()
                            .border_color(rgb(self.tokens.ui.border))
                            .when(mode != PaletteMode::All, |row| {
                                row.child(self.render_command_palette_mode_badge(mode))
                            })
                            .child(self.render_command_palette_icon_slot(
                                LucideIcon::Search,
                                rgb(self.tokens.ui.text_muted),
                            ))
                            .child(div().ml(px(8.0)).flex_1().min_w_0().child(
                                self.render_overlay_query_input(
                                    WorkspaceImeTarget::CommandPalette,
                                    &self.command_palette.raw_query,
                                    query_text,
                                    14.0,
                                    20.0,
                                    cx,
                                ),
                            )),
                    )
                    .child(
                        div()
                            .relative()
                            .id("command-palette-scroll")
                            .w_full()
                            .h(px(rows_height))
                            .max_h(px(COMMAND_PALETTE_LIST_MAX_HEIGHT))
                            .child(tauri_virtual_uniform_list(
                                "command-palette-virtual-list",
                                row_count,
                                self.command_palette.scroll_handle.clone(),
                                TauriVirtualListSpec::new(
                                    px(COMMAND_PALETTE_VIRTUAL_ROW_HEIGHT),
                                    COMMAND_PALETTE_VIRTUAL_OVERSCAN,
                                ),
                                move |range, _window, cx| {
                                    range
                                        .map(|row_index| {
                                            let row = virtual_rows[row_index].clone();
                                            entity.update(cx, |this, cx| {
                                                this.render_command_palette_virtual_row(row, cx)
                                            })
                                        })
                                        .collect::<Vec<_>>()
                                },
                            )),
                    )
                    .when_some(self.command_palette.error.as_ref(), |root, error| {
                        root.child(
                            div()
                                .border_t_1()
                                .border_color(rgb(self.tokens.ui.border))
                                .px(px(12.0))
                                .py(px(8.0))
                                .text_size(px(12.0))
                                .text_color(rgb(self.tokens.ui.error))
                                .child(error.clone()),
                        )
                    })
                    .child(
                        div()
                            .border_t_1()
                            .border_color(rgb(self.tokens.ui.border))
                            .px(px(12.0))
                            .py(px(6.0))
                            .flex()
                            .items_center()
                            .gap(px(12.0))
                            .text_size(px(11.0))
                            .line_height(px(16.0))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::NonSelectable,
                                "command-palette-footer",
                                "hint",
                                self.i18n.t("command_palette.footer_hint"),
                                self.tokens.ui.text_muted,
                                cx,
                            )),
                    ),
            );
        let palette_top = self
            .ai_overlay_window_size
            .map(|(_, height)| height * COMMAND_PALETTE_TOP_RATIO)
            .unwrap_or(COMMAND_PALETTE_FALLBACK_TOP);

        dismissible_command_palette_backdrop()
            .items_start()
            .justify_center()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, window, cx| {
                    this.close_command_palette(cx);
                    window.focus(&this.focus_handle);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _event, window, cx| {
                    this.close_command_palette(cx);
                    window.focus(&this.focus_handle);
                    cx.stop_propagation();
                }),
            )
            .child(
                div()
                    .mt(px(palette_top))
                    .child(overlay_content_boundary(panel)),
            )
            .into_any_element()
    }

    fn render_command_palette_mode_badge(&self, mode: PaletteMode) -> AnyElement {
        div()
            .ml(px(8.0))
            .mr(px(2.0))
            .rounded(px(self.tokens.radii.xs))
            .bg(rgba(
                (self.tokens.ui.accent << 8) | COMMAND_PALETTE_MODE_BADGE_ALPHA,
            ))
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

    fn render_command_palette_section_heading(
        &self,
        section: PaletteSection,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = self.i18n.t(section_label_key(section));
        div()
            .w_full()
            .h(px(COMMAND_PALETTE_VIRTUAL_ROW_HEIGHT))
            .px(px(12.0))
            .flex()
            .items_center()
            .text_size(px(12.0))
            .line_height(px(16.0))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(self.render_display_text_with_role(
                SelectableTextRole::NonSelectable,
                "command-palette-section-heading",
                section_label_key(section),
                label,
                self.tokens.ui.text_muted,
                cx,
            ))
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
        let mut label_row = div().flex().flex_1().items_center().min_w_0().child(
            self.render_highlighted_palette_text(&ranked.item.label, &ranked.highlights, selected),
        );
        if let Some(detail) = ranked.item.detail.as_ref() {
            label_row = label_row.child(
                div()
                    .ml(px(4.0))
                    .min_w_0()
                    .truncate()
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(detail.clone()),
            );
        }
        div()
            .id(("command-palette-row", index))
            // Tauri CommandItem is a full-width flex row; this keeps its
            // CommandShortcut ml-auto at the panel edge instead of after text.
            .w_full()
            .h(px(COMMAND_PALETTE_VIRTUAL_ROW_HEIGHT))
            .px(px(12.0))
            .flex()
            .items_center()
            .gap(px(COMMAND_PALETTE_ITEM_GAP))
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
            .on_mouse_move(
                cx.listener(move |this, _event: &MouseMoveEvent, _window, cx| {
                    if this.command_palette.selected_index != index {
                        this.command_palette.selected_index = index;
                        cx.notify();
                    }
                }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    this.execute_command_palette_item(item.clone(), window, cx);
                    cx.stop_propagation();
                }),
            )
            .child(self.render_command_palette_icon_slot(
                ranked.item.icon,
                if selected {
                    rgb(self.tokens.ui.accent)
                } else {
                    rgb(self.tokens.ui.text_muted)
                },
            ))
            .child(label_row)
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
                        .flex_none()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(shortcut.clone()),
                )
            })
            .into_any_element()
    }

    fn render_command_palette_virtual_row(
        &self,
        row: CommandPaletteVirtualRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match row {
            CommandPaletteVirtualRow::Heading(section) => {
                self.render_command_palette_section_heading(section, cx)
            }
            CommandPaletteVirtualRow::Item { ranked, item_index } => {
                self.render_command_palette_row(&ranked, item_index, cx)
            }
            CommandPaletteVirtualRow::Empty => div()
                .w_full()
                .h(px(COMMAND_PALETTE_VIRTUAL_ROW_HEIGHT))
                .px(px(16.0))
                .flex()
                .items_center()
                .text_size(px(14.0))
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(self.render_selectable_display_text(
                    "command-palette-empty",
                    &self.command_palette.raw_query,
                    self.i18n.t("command_palette.no_results"),
                    self.tokens.ui.text_muted,
                    cx,
                ))
                .into_any_element(),
        }
    }

    fn render_command_palette_icon_slot(&self, icon: LucideIcon, color: Rgba) -> AnyElement {
        div()
            .w(px(COMMAND_PALETTE_ICON_SLOT))
            .h(px(COMMAND_PALETTE_ICON_SLOT))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .child(Self::render_lucide_icon(
                icon,
                COMMAND_PALETTE_ICON_SLOT,
                color,
            ))
            .into_any_element()
    }

    fn render_highlighted_palette_text(
        &self,
        text: &str,
        highlights: &[usize],
        selected: bool,
    ) -> AnyElement {
        let mut label = div()
            .flex()
            .items_center()
            .min_w_0()
            .truncate()
            .text_size(px(14.0))
            .line_height(px(20.0));
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

    pub(super) fn render_shortcuts_modal(&self, cx: &mut Context<Self>) -> AnyElement {
        let categories = self.filtered_shortcut_categories();
        let query_text = if self.shortcuts_modal.query.is_empty() {
            self.i18n.t("shortcuts_modal.search_placeholder")
        } else {
            self.shortcuts_modal.query.clone()
        };
        let shortcut_count = categories
            .iter()
            .map(|category| category.rows.len())
            .sum::<usize>();
        let rows = Arc::new(shortcuts_modal_virtual_rows(categories));
        let row_count = rows.len();
        let rows_height = (row_count as f32 * SHORTCUTS_MODAL_VIRTUAL_ROW_HEIGHT)
            .min(SHORTCUTS_MODAL_LIST_MAX_HEIGHT);
        let virtual_rows = rows.clone();
        let entity = cx.entity();
        dismissible_dialog_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, window, cx| {
                    this.close_shortcuts_modal(cx);
                    window.focus(&this.focus_handle);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _event, window, cx| {
                    this.close_shortcuts_modal(cx);
                    window.focus(&this.focus_handle);
                    cx.stop_propagation();
                }),
            )
            .child(overlay_content_boundary(
                dialog_content(&self.tokens)
                    .w(px(600.0))
                    .child(
                        div()
                            .h(px(44.0))
                            .px(px(16.0))
                            .flex()
                            .items_center()
                            .gap(px(12.0))
                            .border_b_1()
                            .border_color(rgb(self.tokens.ui.border))
                            .child(Self::render_lucide_icon(
                                LucideIcon::Search,
                                16.0,
                                rgb(self.tokens.ui.text_muted),
                            ))
                            .child(self.render_overlay_query_input(
                                WorkspaceImeTarget::ShortcutsModalSearch,
                                &self.shortcuts_modal.query,
                                query_text,
                                self.tokens.metrics.ui_text_sm,
                                20.0,
                                cx,
                            ))
                            .child(
                                div()
                                    .flex_none()
                                    .rounded(px(self.tokens.radii.sm))
                                    .border_1()
                                    .border_color(rgb(self.tokens.ui.border))
                                    .bg(rgb(self.tokens.ui.bg))
                                    .px(px(6.0))
                                    .py(px(2.0))
                                    .font_family(settings_mono_font_family(
                                        self.settings_store.settings(),
                                    ))
                                    .text_size(px(10.0))
                                    .text_color(rgb(self.tokens.ui.text_muted))
                                    .child(if cfg!(target_os = "macos") {
                                        "⌘/".to_string()
                                    } else {
                                        "Ctrl+/".to_string()
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .h(px(rows_height))
                            .max_h(px(SHORTCUTS_MODAL_LIST_MAX_HEIGHT))
                            .px(px(16.0))
                            .py(px(4.0))
                            .child(tauri_virtual_uniform_list(
                                "shortcuts-modal-virtual-list",
                                row_count,
                                self.shortcuts_modal.scroll_handle.clone(),
                                TauriVirtualListSpec::new(
                                    px(SHORTCUTS_MODAL_VIRTUAL_ROW_HEIGHT),
                                    SHORTCUTS_MODAL_VIRTUAL_OVERSCAN,
                                ),
                                move |range, _window, cx| {
                                    range
                                        .map(|row_index| {
                                            let row = virtual_rows[row_index].clone();
                                            entity.update(cx, |this, cx| {
                                                this.render_shortcuts_modal_virtual_row(row, cx)
                                            })
                                        })
                                        .collect::<Vec<_>>()
                                },
                            )),
                    )
                    .child(
                        div()
                            .px(px(16.0))
                            .py(px(12.0))
                            .flex()
                            .items_center()
                            .justify_between()
                            .border_t_1()
                            .border_color(rgb(self.tokens.ui.border))
                            .bg(rgb(self.tokens.ui.bg_panel))
                            .text_size(px(11.0))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("shortcuts_modal.footer_hint"))
                            .child(
                                div()
                                    .font_family(settings_mono_font_family(
                                        self.settings_store.settings(),
                                    ))
                                    .child(format!(
                                        "{} {}",
                                        shortcut_count,
                                        self.i18n.t("shortcuts_modal.shortcut_count")
                                    )),
                            ),
                    ),
            ))
            .into_any_element()
    }

    fn filtered_shortcut_categories(&self) -> Vec<ShortcutModalCategory> {
        let query = self.shortcuts_modal.query.trim().to_lowercase();
        self.shortcut_modal_categories()
            .into_iter()
            .filter_map(|mut category| {
                category.rows.retain(|row| {
                    query.is_empty()
                        || row.label.to_lowercase().contains(&query)
                        || row.shortcut.to_lowercase().contains(&query)
                        || category.title.to_lowercase().contains(&query)
                });
                if category.rows.is_empty() {
                    None
                } else {
                    Some(category)
                }
            })
            .collect()
    }

    fn shortcut_modal_categories(&self) -> Vec<ShortcutModalCategory> {
        let side = crate::keybindings::KeybindingSide::current();
        let overrides = &self.settings_store.settings().keybindings.overrides;
        let binding = |action_id: &str| {
            crate::keybindings::action_definition(action_id).map(|definition| {
                crate::keybindings::format_combo(&crate::keybindings::effective_combo(
                    definition, overrides, side,
                ))
            })
        };
        let registry_row = |action_id: &str, label_key: &str| {
            binding(action_id).map(|shortcut| ShortcutModalRow {
                label: self.i18n.t(label_key),
                shortcut,
            })
        };
        let tab_range = binding("app.goToTab1").map(|shortcut| ShortcutModalRow {
            label: self.i18n.t("settings_view.help.shortcut_go_to_tab"),
            shortcut: format!("{}1-9", shortcut.trim_end_matches('1')),
        });
        let pane_nav = binding("split.navLeft").map(|shortcut| ShortcutModalRow {
            label: self.i18n.t("settings_view.help.shortcut_nav_pane"),
            shortcut: format!("{}Arrow", shortcut.trim_end_matches('←')),
        });

        let mut categories = vec![
            ShortcutModalCategory::new(
                self.i18n.t("settings_view.help.category_app"),
                [
                    registry_row("app.newTerminal", "settings_view.help.shortcut_new_tab"),
                    registry_row(
                        "app.shellLauncher",
                        "settings_view.help.shortcut_shell_launcher",
                    ),
                    registry_row("app.closeTab", "settings_view.help.shortcut_close_tab"),
                    registry_row(
                        "app.closeOtherTabs",
                        "settings_view.help.shortcut_close_other_tabs",
                    ),
                    registry_row("app.nextTab", "settings_view.help.shortcut_next_tab"),
                    registry_row("app.prevTab", "settings_view.help.shortcut_prev_tab"),
                    tab_range,
                    registry_row(
                        "app.newConnection",
                        "settings_view.help.shortcut_new_connection",
                    ),
                    registry_row("app.navBack", "settings_view.help.shortcut_nav_back"),
                    registry_row("app.navForward", "settings_view.help.shortcut_nav_forward"),
                    registry_row(
                        "app.commandPalette",
                        "settings_view.help.shortcut_command_palette",
                    ),
                    registry_row(
                        "app.toggleSidebar",
                        "settings_view.help.shortcut_toggle_sidebar",
                    ),
                    registry_row("app.settings", "settings_view.help.shortcut_settings"),
                    registry_row("app.zenMode", "settings_view.help.shortcut_zen_mode"),
                    registry_row(
                        "app.showShortcuts",
                        "settings_view.help.shortcut_keyboard_shortcuts",
                    ),
                    registry_row(
                        "app.fontIncrease",
                        "settings_view.help.shortcut_font_increase",
                    ),
                    registry_row(
                        "app.fontDecrease",
                        "settings_view.help.shortcut_font_decrease",
                    ),
                    registry_row("app.fontReset", "settings_view.help.shortcut_font_reset"),
                ]
                .into_iter()
                .flatten()
                .collect(),
            ),
            ShortcutModalCategory::new(
                self.i18n.t("settings_view.help.category_terminal"),
                [
                    registry_row("terminal.search", "settings_view.help.shortcut_find"),
                    registry_row("terminal.paste", "settings_view.help.shortcut_paste"),
                    registry_row("terminal.aiPanel", "settings_view.help.shortcut_ai_panel"),
                    registry_row(
                        "terminal.recording",
                        "settings_view.help.shortcut_recording",
                    ),
                    registry_row(
                        "terminal.closePanel",
                        "settings_view.help.shortcut_close_panel",
                    ),
                ]
                .into_iter()
                .flatten()
                .collect(),
            ),
            ShortcutModalCategory::new(
                self.i18n.t("settings_view.help.category_split"),
                [
                    registry_row("split.horizontal", "settings_view.help.shortcut_split_h"),
                    registry_row("split.vertical", "settings_view.help.shortcut_split_v"),
                    registry_row("split.closePane", "settings_view.help.shortcut_close_pane"),
                    pane_nav,
                ]
                .into_iter()
                .flatten()
                .collect(),
            ),
        ];

        for (category_key, shortcut_rows) in shortcut_reference_rows() {
            categories.push(ShortcutModalCategory::new(
                self.i18n.t(category_key),
                shortcut_rows
                    .into_iter()
                    .map(|(label_key, mac, other)| ShortcutModalRow {
                        label: self.i18n.t(label_key),
                        shortcut: if cfg!(target_os = "macos") {
                            mac.to_string()
                        } else {
                            other.to_string()
                        },
                    })
                    .collect(),
            ));
        }

        categories.push(ShortcutModalCategory::new(
            self.i18n.t("settings_view.help.category_palette"),
            [
                registry_row("palette.eventLog", "settings_view.help.shortcut_event_log"),
                registry_row(
                    "palette.aiSidebar",
                    "settings_view.help.shortcut_ai_sidebar",
                ),
                registry_row("palette.broadcast", "settings_view.help.shortcut_broadcast"),
            ]
            .into_iter()
            .flatten()
            .collect(),
        ));

        categories
    }

    fn render_shortcut_modal_row(
        &self,
        row: ShortcutModalRow,
        show_separator: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Tauri KeyboardShortcutsModal renders each item as
        // `flex items-center justify-between`: the label owns the left side and
        // the kbd badge stays pinned to the row's right edge. Keep the explicit
        // full-width/flex split here so GPUI virtual rows do not shrink to
        // content and pull shortcuts next to the text.
        div()
            .w_full()
            .h(px(SHORTCUTS_MODAL_VIRTUAL_ROW_HEIGHT))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(16.0))
            .when(show_separator, |item| {
                item.border_b_1()
                    .border_color(rgba((self.tokens.ui.border << 8) | 0x33))
            })
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.render_selectable_display_text(
                        "shortcuts-modal-label",
                        &row.shortcut,
                        row.label.clone(),
                        self.tokens.ui.text,
                        cx,
                    )),
            )
            .child(
                div()
                    .flex_none()
                    .ml_auto()
                    .rounded(px(self.tokens.radii.sm))
                    .border_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .bg(rgb(self.tokens.ui.bg_panel))
                    .px(px(8.0))
                    .py(px(2.0))
                    .font_family(settings_mono_font_family(self.settings_store.settings()))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.render_selectable_display_text(
                        "shortcuts-modal-shortcut",
                        &row.label,
                        row.shortcut.clone(),
                        self.tokens.ui.text_muted,
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn render_shortcuts_modal_virtual_row(
        &self,
        row: ShortcutsModalVirtualRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match row {
            ShortcutsModalVirtualRow::Heading(title) => div()
                .h(px(SHORTCUTS_MODAL_VIRTUAL_ROW_HEIGHT))
                .flex()
                .items_center()
                .gap(px(6.0))
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(Self::render_lucide_icon(
                    LucideIcon::Keyboard,
                    12.0,
                    rgb(self.tokens.ui.text_muted),
                ))
                .child(self.render_selectable_display_text(
                    "shortcuts-modal-heading",
                    &title,
                    title.to_uppercase(),
                    self.tokens.ui.text_muted,
                    cx,
                ))
                .into_any_element(),
            ShortcutsModalVirtualRow::Row {
                row,
                show_separator,
            } => self.render_shortcut_modal_row(row, show_separator, cx),
            ShortcutsModalVirtualRow::Empty => div()
                .h(px(SHORTCUTS_MODAL_VIRTUAL_ROW_HEIGHT))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(self.tokens.metrics.ui_text_sm))
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(self.render_selectable_display_text(
                    "shortcuts-modal-empty",
                    "shortcuts_modal.no_results",
                    self.i18n.t("shortcuts_modal.no_results"),
                    self.tokens.ui.text_muted,
                    cx,
                ))
                .into_any_element(),
        }
    }
}

#[derive(Clone)]
struct ShortcutModalCategory {
    title: String,
    rows: Vec<ShortcutModalRow>,
}

impl ShortcutModalCategory {
    fn new(title: String, rows: Vec<ShortcutModalRow>) -> Self {
        Self { title, rows }
    }
}

#[derive(Clone)]
struct ShortcutModalRow {
    label: String,
    shortcut: String,
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

fn command_palette_scroll_child_index(
    ranked_items: &[RankedItem],
    selected_index: usize,
) -> Option<usize> {
    let mut previous_section = None;
    let mut child_index = 0;
    for (item_index, ranked) in ranked_items.iter().enumerate() {
        if previous_section != Some(ranked.item.section) {
            previous_section = Some(ranked.item.section);
            child_index += 1;
        }
        if item_index == selected_index {
            return Some(child_index);
        }
        child_index += 1;
    }
    None
}

fn command_palette_virtual_rows(ranked_items: Vec<RankedItem>) -> Vec<CommandPaletteVirtualRow> {
    if ranked_items.is_empty() {
        return vec![CommandPaletteVirtualRow::Empty];
    }

    let mut rows = Vec::new();
    let mut previous_section = None;
    for (index, ranked) in ranked_items.into_iter().enumerate() {
        if previous_section != Some(ranked.item.section) {
            previous_section = Some(ranked.item.section);
            rows.push(CommandPaletteVirtualRow::Heading(ranked.item.section));
        }
        rows.push(CommandPaletteVirtualRow::Item {
            ranked,
            item_index: index,
        });
    }
    rows
}

fn shortcuts_modal_virtual_rows(
    categories: Vec<ShortcutModalCategory>,
) -> Vec<ShortcutsModalVirtualRow> {
    if categories.is_empty() {
        return vec![ShortcutsModalVirtualRow::Empty];
    }

    let mut rows = Vec::new();
    for category in categories {
        rows.push(ShortcutsModalVirtualRow::Heading(category.title));
        let row_count = category.rows.len();
        for (index, row) in category.rows.into_iter().enumerate() {
            rows.push(ShortcutsModalVirtualRow::Row {
                row,
                show_separator: index + 1 < row_count,
            });
        }
    }
    rows
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
        PaletteSection::QuickConnect => "command_palette.quick_connect",
        PaletteSection::Recent => "command_palette.section_recent",
        PaletteSection::Commands => "command_palette.section_commands",
        PaletteSection::Sessions => "command_palette.section_sessions",
        PaletteSection::Connections => "command_palette.section_connections",
        PaletteSection::Plugins => "command_palette.section_plugins",
        PaletteSection::Help => "command_palette.section_help",
    }
}

fn command_palette_placeholder_key(mode: PaletteMode) -> &'static str {
    match mode {
        PaletteMode::All => "command_palette.placeholder",
        PaletteMode::Commands => "command_palette.placeholder_commands",
        PaletteMode::Sessions => "command_palette.placeholder_sessions",
        PaletteMode::Connections => "command_palette.placeholder_connections",
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
        TabKind::Plugin { .. } => LucideIcon::Puzzle,
        TabKind::CloudSync => LucideIcon::Cloud,
        TabKind::Settings => LucideIcon::Settings,
        TabKind::SessionManager => LucideIcon::LayoutList,
    }
}

fn keybinding_command(
    id: &'static str,
    label_key: &'static str,
    action_id: &'static str,
    icon: LucideIcon,
) -> CommandSpec {
    CommandSpec {
        id,
        label_key: label_key.into(),
        icon,
        shortcut_action: Some(action_id),
        action: PaletteAction::Keybinding(action_id),
    }
}

fn command_palette_specs() -> Vec<CommandSpec> {
    vec![
        keybinding_command(
            "cmd:new_terminal",
            "command_palette.cmd_new_terminal",
            "app.newTerminal",
            LucideIcon::Terminal,
        ),
        keybinding_command(
            "cmd:new_connection",
            "command_palette.cmd_new_connection",
            "app.newConnection",
            LucideIcon::Plus,
        ),
        keybinding_command(
            "cmd:settings",
            "command_palette.cmd_settings",
            "app.settings",
            LucideIcon::Settings,
        ),
        keybinding_command(
            "cmd:toggle_sidebar",
            "command_palette.cmd_toggle_sidebar",
            "app.toggleSidebar",
            LucideIcon::PanelLeft,
        ),
        keybinding_command(
            "cmd:zen_mode",
            "command_palette.cmd_zen_mode",
            "app.zenMode",
            LucideIcon::AppWindow,
        ),
        keybinding_command(
            "cmd:toggle_panel",
            "command_palette.cmd_toggle_panel",
            "palette.eventLog",
            LucideIcon::LayoutList,
        ),
        keybinding_command(
            "cmd:toggle_ai_sidebar",
            "command_palette.cmd_toggle_ai_sidebar",
            "palette.aiSidebar",
            LucideIcon::PanelLeft,
        ),
        keybinding_command(
            "cmd:close_tab",
            "command_palette.cmd_close_tab",
            "app.closeTab",
            LucideIcon::X,
        ),
        keybinding_command(
            "cmd:split_horizontal",
            "command_palette.cmd_split_horizontal",
            "split.horizontal",
            LucideIcon::SplitSquareHorizontal,
        ),
        keybinding_command(
            "cmd:split_vertical",
            "command_palette.cmd_split_vertical",
            "split.vertical",
            LucideIcon::SplitSquareVertical,
        ),
        keybinding_command(
            "cmd:broadcast_toggle",
            "command_palette.cmd_broadcast_toggle",
            "palette.broadcast",
            LucideIcon::Radio,
        ),
        keybinding_command(
            "cmd:next_tab",
            "command_palette.cmd_next_tab",
            "app.nextTab",
            LucideIcon::ChevronRight,
        ),
        keybinding_command(
            "cmd:prev_tab",
            "command_palette.cmd_prev_tab",
            "app.prevTab",
            LucideIcon::ChevronLeft,
        ),
        keybinding_command(
            "cmd:close_other_tabs",
            "command_palette.cmd_close_other_tabs",
            "app.closeOtherTabs",
            LucideIcon::Layers,
        ),
        CommandSpec {
            id: "cmd:close_all_tabs",
            label_key: "command_palette.cmd_close_all_tabs".into(),
            icon: LucideIcon::Layers,
            shortcut_action: None,
            action: PaletteAction::CloseAllTabs,
        },
        keybinding_command(
            "cmd:go_back",
            "command_palette.cmd_go_back",
            "app.navBack",
            LucideIcon::ArrowDownRight,
        ),
        keybinding_command(
            "cmd:go_forward",
            "command_palette.cmd_go_forward",
            "app.navForward",
            LucideIcon::ArrowRight,
        ),
        CommandSpec {
            id: "cmd:open_connection_manager",
            label_key: "command_palette.cmd_open_connection_manager".into(),
            icon: LucideIcon::FolderOpen,
            shortcut_action: None,
            action: PaletteAction::OpenSessionManager,
        },
        CommandSpec {
            id: "cmd:theme_next",
            label_key: "command_palette.cmd_theme_next".into(),
            icon: LucideIcon::Sparkles,
            shortcut_action: None,
            action: PaletteAction::ThemeNext(true),
        },
        CommandSpec {
            id: "cmd:theme_prev",
            label_key: "command_palette.cmd_theme_prev".into(),
            icon: LucideIcon::Sparkles,
            shortcut_action: None,
            action: PaletteAction::ThemeNext(false),
        },
        keybinding_command(
            "cmd:font_increase",
            "command_palette.cmd_font_increase",
            "app.fontIncrease",
            LucideIcon::Plus,
        ),
        keybinding_command(
            "cmd:font_decrease",
            "command_palette.cmd_font_decrease",
            "app.fontDecrease",
            LucideIcon::ArrowDown,
        ),
        keybinding_command(
            "cmd:font_reset",
            "command_palette.cmd_font_reset",
            "app.fontReset",
            LucideIcon::RotateCcw,
        ),
        CommandSpec {
            id: "cmd:cursor_block",
            label_key: "command_palette.cmd_cursor_block".into(),
            icon: LucideIcon::Square,
            shortcut_action: None,
            action: PaletteAction::CursorStyle(SettingsCursorStyle::Block),
        },
        CommandSpec {
            id: "cmd:cursor_bar",
            label_key: "command_palette.cmd_cursor_bar".into(),
            icon: LucideIcon::Terminal,
            shortcut_action: None,
            action: PaletteAction::CursorStyle(SettingsCursorStyle::Bar),
        },
        CommandSpec {
            id: "cmd:cursor_underline",
            label_key: "command_palette.cmd_cursor_underline".into(),
            icon: LucideIcon::ArrowDown,
            shortcut_action: None,
            action: PaletteAction::CursorStyle(SettingsCursorStyle::Underline),
        },
        CommandSpec {
            id: "cmd:sidebar_sessions",
            label_key: "command_palette.cmd_sidebar_sessions".into(),
            icon: LucideIcon::ListTree,
            shortcut_action: None,
            action: PaletteAction::Sidebar(SidebarSection::Sessions),
        },
        CommandSpec {
            id: "cmd:sidebar_saved",
            label_key: "command_palette.cmd_sidebar_saved".into(),
            icon: LucideIcon::Server,
            shortcut_action: None,
            action: PaletteAction::OpenSavedConnections,
        },
        CommandSpec {
            id: "cmd:sidebar_sftp",
            label_key: "command_palette.cmd_sidebar_sftp".into(),
            icon: LucideIcon::HardDrive,
            shortcut_action: None,
            action: PaletteAction::Sidebar(SidebarSection::Terminal),
        },
        CommandSpec {
            id: "cmd:sidebar_forwards",
            label_key: "command_palette.cmd_sidebar_forwards".into(),
            icon: LucideIcon::ArrowLeftRight,
            shortcut_action: None,
            action: PaletteAction::Sidebar(SidebarSection::Activity),
        },
        CommandSpec {
            id: "cmd:sidebar_connections",
            label_key: "command_palette.cmd_sidebar_connections".into(),
            icon: LucideIcon::Network,
            shortcut_action: None,
            action: PaletteAction::Sidebar(SidebarSection::Network),
        },
        CommandSpec {
            id: "cmd:sidebar_ai",
            label_key: "command_palette.cmd_sidebar_ai".into(),
            icon: LucideIcon::Bot,
            shortcut_action: None,
            action: PaletteAction::Keybinding("palette.aiSidebar"),
        },
        CommandSpec {
            id: "cmd:open_connection_pool",
            label_key: "command_palette.cmd_open_connection_pool".into(),
            icon: LucideIcon::Activity,
            shortcut_action: None,
            action: PaletteAction::OpenConnectionPool,
        },
        CommandSpec {
            id: "cmd:disconnect_all",
            label_key: "command_palette.cmd_disconnect_all".into(),
            icon: LucideIcon::Power,
            shortcut_action: None,
            action: PaletteAction::DisconnectAll,
        },
        CommandSpec {
            id: "cmd:reconnect_all",
            label_key: "command_palette.cmd_reconnect_all".into(),
            icon: LucideIcon::RefreshCw,
            shortcut_action: None,
            action: PaletteAction::ReconnectAll,
        },
        CommandSpec {
            id: "cmd:cancel_reconnect",
            label_key: "command_palette.cmd_cancel_reconnect".into(),
            icon: LucideIcon::StopCircle,
            shortcut_action: None,
            action: PaletteAction::CancelReconnect,
        },
        CommandSpec {
            id: "cmd:health_check",
            label_key: "command_palette.cmd_health_check".into(),
            icon: LucideIcon::Activity,
            shortcut_action: None,
            action: PaletteAction::HealthCheck,
        },
        CommandSpec {
            id: "cmd:shell_launcher",
            label_key: "command_palette.cmd_shell_launcher".into(),
            icon: LucideIcon::Terminal,
            shortcut_action: Some("app.shellLauncher"),
            action: PaletteAction::Keybinding("app.shellLauncher"),
        },
        CommandSpec {
            id: "cmd:detach_terminal",
            label_key: "command_palette.cmd_detach_terminal".into(),
            icon: LucideIcon::Archive,
            shortcut_action: None,
            action: PaletteAction::DetachTerminal,
        },
        CommandSpec {
            id: "cmd:cleanup_dead",
            label_key: "command_palette.cmd_cleanup_dead".into(),
            icon: LucideIcon::Trash2,
            shortcut_action: None,
            action: PaletteAction::CleanupDead,
        },
        CommandSpec {
            id: "cmd:toggle_fps",
            label_key: "command_palette.cmd_toggle_fps".into(),
            icon: LucideIcon::Gauge,
            shortcut_action: None,
            action: PaletteAction::ToggleFps,
        },
        keybinding_command(
            "cmd:close_pane",
            "command_palette.cmd_close_pane",
            "split.closePane",
            LucideIcon::PanelLeftClose,
        ),
        keybinding_command(
            "cmd:focus_next_pane",
            "command_palette.cmd_focus_next_pane",
            "split.navRight",
            LucideIcon::CornerDownLeft,
        ),
        CommandSpec {
            id: "cmd:reset_panes",
            label_key: "command_palette.cmd_reset_panes".into(),
            icon: LucideIcon::Layers,
            shortcut_action: None,
            action: PaletteAction::ResetPanes,
        },
        CommandSpec {
            id: "cmd:open_plugin_manager",
            label_key: "command_palette.cmd_open_plugin_manager".into(),
            icon: LucideIcon::Puzzle,
            shortcut_action: None,
            action: PaletteAction::OpenPluginManager,
        },
        CommandSpec {
            id: "cmd:open_cloud_sync",
            label_key: "command_palette.cmd_open_cloud_sync".into(),
            icon: LucideIcon::Cloud,
            shortcut_action: None,
            action: PaletteAction::OpenCloudSync,
        },
        CommandSpec {
            id: "cmd:open_topology",
            label_key: "command_palette.cmd_open_topology".into(),
            icon: LucideIcon::Network,
            shortcut_action: None,
            action: PaletteAction::OpenTopology,
        },
        CommandSpec {
            id: "cmd:reset_settings",
            label_key: "command_palette.cmd_reset_settings".into(),
            icon: LucideIcon::AlertTriangle,
            shortcut_action: None,
            action: PaletteAction::ResetSettings,
        },
    ]
}

fn help_palette_specs() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: "cmd:show_shortcuts",
        label_key: "command_palette.cmd_show_shortcuts".into(),
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
