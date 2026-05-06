use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Datelike, Local, Utc};
use gpui::{StatefulInteractiveElement, prelude::*};
use oxideterm_connections::{
    AuthType, ConnectionInfo, SaveConnectionRequest, SavedAuth, SavedConnection, SshConfigHost,
    list_ssh_config_hosts, resolve_ssh_config_alias,
};
use oxideterm_gpui_ui::{
    button::{ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, button_with},
    checkbox,
    text_input::{text_caret, text_input_anchor_probe},
};
use oxideterm_ssh::AuthMethod;

use super::*;
use crate::workspace::ime::WorkspaceImeTarget;

const UNGROUPED_FILTER: &str = "__ungrouped__";
const RECENT_FILTER: &str = "__recent__";
const IMPORTED_GROUP: &str = "Imported";
const SSH_CONFIG_TAG: &str = "ssh-config";
const BG_ACTIVE_THEME_ALPHA: u32 = 0x66; // Tauri [data-bg-active] color-mix(... 40%, transparent)
const BG_ACTIVE_HOVER_ALPHA: u32 = 0x80; // Tauri bg-hover 50%
const BG_ACTIVE_BORDER_ALPHA: u32 = 0xbf; // Tauri border 75%
const BG_ACTIVE_BORDER_HALF_ALPHA: u32 = 0x60; // Tauri border/50 after active border mix
const BG_ACTIVE_ROW_SELECTED_ALPHA: u32 = 0x1a; // Tauri blue-500/10
const MANAGER_FOLDER_TREE_WIDTH: f32 = 180.0; // Tauri w-[180px]
const MANAGER_TOOLBAR_SEARCH_WIDTH: f32 = 384.0; // Tauri max-w-sm
const MANAGER_COL_CHECKBOX: f32 = 32.0;
const MANAGER_COL_NAME_BASIS: f32 = 140.0;
const MANAGER_COL_NAME_MIN: f32 = 100.0;
const MANAGER_COL_HOST: f32 = 130.0;
const MANAGER_COL_PORT: f32 = 50.0;
const MANAGER_COL_USERNAME: f32 = 90.0;
const MANAGER_COL_AUTH: f32 = 72.0;
const MANAGER_COL_GROUP: f32 = 100.0;
const MANAGER_COL_LAST_USED: f32 = 90.0;
const MANAGER_COL_ACTIONS: f32 = 84.0;
const MANAGER_COLOR_INDICATOR_WIDTH: f32 = 4.0;
const MANAGER_ROW_TEXT_SIZE: f32 = 14.0;
const MANAGER_ROW_META_TEXT_SIZE: f32 = 12.0;
const MANAGER_TABLE_HEADER_TEXT_SIZE: f32 = 12.0;
const MANAGER_AUTH_BADGE_TEXT_SIZE: f32 = 10.0;
const MANAGER_AUTH_BADGE_ICON_SIZE: f32 = 12.0; // Tauri h-3 w-3
const MANAGER_AUTH_BADGE_GAP: f32 = 4.0; // Tauri gap-1
const MANAGER_AUTH_BADGE_PADDING_X: f32 = 6.0; // Tauri px-1.5
const MANAGER_AUTH_BADGE_CHAR_WIDTH: f32 = 6.0; // Approx text-[10px] inline span width
const MANAGER_ROW_ACTION_BUTTON: f32 = 24.0; // Tauri h-6 w-6
const MANAGER_ROW_MORE_BUTTON: f32 = 28.0; // Tauri h-7 w-7
const MANAGER_ROW_MENU_WIDTH: f32 = 184.0;
const MANAGER_ROW_MENU_HEIGHT: f32 = 112.0;
const MANAGER_ROW_CONTEXT_MENU_HEIGHT: f32 = 180.0;
const MANAGER_RESPONSIVE_SM: f32 = 640.0;
const MANAGER_RESPONSIVE_MD: f32 = 768.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(super) enum SessionManagerInput {
    Search,
    SavedSearch,
    NewGroup,
}

impl SessionManagerInput {
    pub(super) fn anchor_key(self) -> u64 {
        match self {
            Self::Search => 1,
            Self::SavedSearch => 2,
            Self::NewGroup => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SessionSortField {
    Name,
    Host,
    Port,
    Username,
    AuthType,
    Group,
    LastUsed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SortDirection {
    Asc,
    Desc,
}

impl SortDirection {
    fn toggled(self) -> Self {
        match self {
            Self::Asc => Self::Desc,
            Self::Desc => Self::Asc,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TableCellStyle {
    Primary,
    Meta,
    MetaMono,
}

#[derive(Clone, Debug)]
pub(super) struct SessionManagerState {
    pub(super) selected_group: Option<String>,
    pub(super) search_query: String,
    pub(super) saved_search_query: String,
    pub(super) sort_field: SessionSortField,
    pub(super) sort_direction: SortDirection,
    pub(super) selected_ids: HashSet<String>,
    pub(super) hovered_connection_id: Option<String>,
    pub(super) row_menu_connection_id: Option<String>,
    pub(super) row_menu_opens_above: bool,
    pub(super) row_context_menu_connection_id: Option<String>,
    pub(super) row_context_menu_x: f32,
    pub(super) row_context_menu_y: f32,
    pub(super) expanded_groups: HashSet<String>,
    pub(super) focused_input: Option<SessionManagerInput>,
    pub(super) show_new_group: bool,
    pub(super) new_group_name: String,
    pub(super) show_import: bool,
    pub(super) ssh_config_hosts: Vec<SshConfigHost>,
    pub(super) selected_import_aliases: HashSet<String>,
    pub(super) show_batch_move: bool,
    pub(super) status: Option<String>,
}

impl Default for SessionManagerState {
    fn default() -> Self {
        Self {
            selected_group: None,
            search_query: String::new(),
            saved_search_query: String::new(),
            sort_field: SessionSortField::LastUsed,
            sort_direction: SortDirection::Desc,
            selected_ids: HashSet::new(),
            hovered_connection_id: None,
            row_menu_connection_id: None,
            row_menu_opens_above: false,
            row_context_menu_connection_id: None,
            row_context_menu_x: 0.0,
            row_context_menu_y: 0.0,
            expanded_groups: HashSet::new(),
            focused_input: None,
            show_new_group: false,
            new_group_name: String::new(),
            show_import: false,
            ssh_config_hosts: Vec::new(),
            selected_import_aliases: HashSet::new(),
            show_batch_move: false,
            status: None,
        }
    }
}

impl WorkspaceApp {
    pub(super) fn render_session_manager_surface(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let has_background = self
            .terminal_background_preferences("session_manager")
            .is_some();
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(if has_background {
                rgba(0x00000000)
            } else {
                rgb(theme.bg)
            })
            .text_color(rgb(theme.text))
            .child(self.render_session_manager_toolbar(window, has_background, cx))
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .flex()
                    .flex_row()
                    .child(self.render_session_manager_folder_tree(has_background, cx))
                    .child(self.render_session_manager_table(has_background, cx)),
            )
            .when_some(self.session_manager.status.clone(), |surface, status| {
                surface.child(
                    div()
                        .h(px(32.0))
                        .flex()
                        .items_center()
                        .px_4()
                        .border_t_1()
                        .border_color(theme_border(theme.border, has_background))
                        .bg(theme_bg(theme.bg, has_background))
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .text_color(rgb(theme.accent))
                        .child(status),
                )
            })
            .when(self.session_manager.show_new_group, |surface| {
                surface.child(self.render_new_group_dialog(cx))
            })
            .when(self.session_manager.show_import, |surface| {
                surface.child(self.render_ssh_config_import_dialog(cx))
            })
            .when_some(
                self.session_manager
                    .row_context_menu_connection_id
                    .as_deref()
                    .and_then(|id| self.connection_info_by_id(id)),
                |surface, conn| {
                    surface.child(self.render_row_context_menu(conn, window, has_background, cx))
                },
            )
            .into_any_element()
    }

    pub(super) fn handle_session_manager_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(input) = self.session_manager.focused_input else {
            return false;
        };
        let key = event.keystroke.key.as_str();
        if event.keystroke.modifiers.platform || event.keystroke.modifiers.control {
            return false;
        }
        match key {
            "escape" => {
                self.session_manager.focused_input = None;
                self.ime_marked_text = None;
                cx.notify();
                true
            }
            "enter" if input == SessionManagerInput::NewGroup => {
                self.create_session_group(cx);
                true
            }
            "backspace" => {
                match input {
                    SessionManagerInput::Search => self.session_manager.search_query.pop(),
                    SessionManagerInput::SavedSearch => {
                        self.session_manager.saved_search_query.pop()
                    }
                    SessionManagerInput::NewGroup => self.session_manager.new_group_name.pop(),
                };
                if input == SessionManagerInput::Search {
                    self.clear_session_selection_for_invisible_rows();
                }
                cx.notify();
                true
            }
            _ => false,
        }
    }

    pub(super) fn open_session_manager_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tab_id = if let Some(tab) = self
            .tabs
            .iter()
            .find(|tab| tab.kind == TabKind::SessionManager)
        {
            tab.id
        } else {
            let tab_id = self.alloc_tab_id();
            self.tabs.push(Tab {
                id: tab_id,
                kind: TabKind::SessionManager,
                title: self.i18n.t("sessionManager.title"),
                title_source: TabTitleSource::I18nKey("sessionManager.title"),
                root_pane: None,
                active_pane_id: None,
            });
            tab_id
        };
        self.active_tab_id = Some(tab_id);
        self.active_surface = ActiveSurface::Terminal;
        self.active_sidebar_section = SidebarSection::Connections;
        self.needs_active_pane_focus = false;
        if self.sidebar_collapsed {
            self.sidebar_collapsed = false;
        }
        window.focus(&self.focus_handle);
        self.reveal_active_tab(window);
        self.persist_sidebar_settings();
        cx.notify();
    }

    fn render_session_manager_toolbar(
        &self,
        window: &Window,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let selected_count = self.session_manager.selected_ids.len();
        let viewport_width = f32::from(window.viewport_size().width);
        let show_primary_labels = viewport_width >= MANAGER_RESPONSIVE_SM;
        let show_transfer_labels = viewport_width >= MANAGER_RESPONSIVE_MD;
        div()
            .min_h(px(48.0))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.0))
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(theme_border(theme.border, has_background))
            .bg(theme_bg(theme.bg, has_background))
            .child(
                div()
                    .flex_1()
                    .min_w(px(160.0))
                    .max_w(px(MANAGER_TOOLBAR_SEARCH_WIDTH))
                    .child(self.render_session_text_input(
                        SessionManagerInput::Search,
                        &self.session_manager.search_query,
                        self.i18n.t("sessionManager.toolbar.search_placeholder"),
                        cx,
                    )),
            )
            .child(
                self.render_toolbar_button(
                    LucideIcon::Plus,
                    self.i18n.t("sessionManager.toolbar.new_connection"),
                    ButtonVariant::Default,
                    has_background,
                    show_primary_labels,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, window, cx| {
                        this.open_new_connection_form(window, cx);
                        cx.stop_propagation();
                    }),
                ),
            )
            .child(
                self.render_toolbar_button(
                    LucideIcon::Network,
                    self.i18n.t("sessionManager.toolbar.auto_route"),
                    ButtonVariant::Outline,
                    has_background,
                    show_primary_labels,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_this, _event, _window, cx| cx.stop_propagation()),
                ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .when(selected_count > 0, |batch| {
                        batch
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .px_1()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(theme.text_muted))
                                    .child(selected_count_label(&self.i18n, selected_count)),
                            )
                            .child(
                                self.render_session_manager_button(
                                    LucideIcon::FolderInput,
                                    self.i18n.t("sessionManager.batch.move_to_group"),
                                    ButtonVariant::Outline,
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.session_manager.show_batch_move =
                                            !this.session_manager.show_batch_move;
                                        cx.notify();
                                        cx.stop_propagation();
                                    }),
                                ),
                            )
                            .child(
                                self.render_session_manager_button(
                                    LucideIcon::Trash2,
                                    self.i18n.t("sessionManager.batch.delete"),
                                    ButtonVariant::Outline,
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.delete_selected_connections(cx);
                                        cx.stop_propagation();
                                    }),
                                ),
                            )
                    }),
            )
            .when(
                selected_count > 0 && self.session_manager.show_batch_move,
                |toolbar| toolbar.child(self.render_batch_move_popover(cx)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(22.0))
                    .child(self.render_toolbar_link_icon(
                        LucideIcon::Download,
                        "sessionManager.toolbar.import",
                        true,
                        show_transfer_labels,
                        cx,
                    ))
                    .child(self.render_toolbar_link_icon(
                        LucideIcon::Upload,
                        "sessionManager.toolbar.export",
                        false,
                        show_transfer_labels,
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn render_session_manager_folder_tree(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let all_count = self.connection_store.connections().len();
        let ungrouped_count = self
            .connection_store
            .connections()
            .iter()
            .filter(|conn| conn.group.is_none())
            .count();
        let (root_groups, child_groups) = self.session_group_tree();
        let mut groups = div()
            .id("session-manager-folder-tree-scroll")
            .flex_1()
            .min_h(px(0.0))
            .min_w(px(0.0))
            .overflow_y_scroll()
            .px_1()
            .py_1();

        for group in root_groups {
            groups = groups.child(self.render_group_tree_node(
                group,
                0,
                &child_groups,
                has_background,
                cx,
            ));
        }

        if ungrouped_count > 0 {
            groups = groups.child(self.render_group_tree_item(
                Some(UNGROUPED_FILTER.to_string()),
                LucideIcon::Folder,
                self.i18n.t("sessionManager.folder_tree.ungrouped"),
                Some(ungrouped_count),
                0,
                has_background,
                cx,
            ));
        }

        div()
            .id("session-manager-folder-tree")
            .w(px(MANAGER_FOLDER_TREE_WIDTH))
            .min_w(px(140.0))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(theme_border(theme.border, has_background))
            .bg(if has_background {
                rgba(0x00000000)
            } else {
                rgb(theme.bg)
            })
            .child(
                div()
                    .flex_none()
                    .pt_2()
                    .px_1()
                    .child(self.render_group_tree_item(
                        None,
                        LucideIcon::Inbox,
                        self.i18n.t("sessionManager.folder_tree.all_connections"),
                        Some(all_count),
                        0,
                        has_background,
                        cx,
                    ))
                    .child(self.render_new_group_tree_item(has_background, cx)),
            )
            .child(groups)
            .child(
                div()
                    .flex_none()
                    .border_t_1()
                    .border_color(theme_border(theme.border, has_background))
                    .px_1()
                    .py(px(6.0))
                    .child(self.render_group_tree_item(
                        Some(RECENT_FILTER.to_string()),
                        LucideIcon::Clock,
                        self.i18n.t("sessionManager.folder_tree.recent"),
                        None,
                        0,
                        has_background,
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn render_group_tree_item(
        &self,
        group: Option<String>,
        icon: LucideIcon,
        label: String,
        count: Option<usize>,
        depth: usize,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active = self.session_manager.selected_group == group;
        div()
            .min_h(px(32.0))
            .pl(px(12.0 + depth as f32 * 16.0))
            .pr_2()
            .flex()
            .items_center()
            .gap(px(6.0))
            .rounded(px(self.tokens.radii.md))
            .cursor_pointer()
            .bg(if active {
                theme_active_bg(theme.bg_active, has_background)
            } else {
                rgba(0x00000000)
            })
            .text_color(if active {
                rgb(theme.text)
            } else {
                rgb(theme.text)
            })
            .hover(move |item| {
                if active {
                    item
                } else {
                    item.bg(theme_hover_bg(theme.bg_hover, has_background))
                }
            })
            .child(Self::render_lucide_icon(
                icon,
                16.0,
                match icon {
                    LucideIcon::Inbox => rgb(0x60a5fa),
                    LucideIcon::Folder | LucideIcon::FolderOpen => rgb(0xeab308),
                    _ if active => rgb(theme.text),
                    _ => rgb(theme.text_muted),
                },
            ))
            .child(
                div()
                    .flex_1()
                    .truncate()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(if active {
                        gpui::FontWeight::MEDIUM
                    } else {
                        gpui::FontWeight::NORMAL
                    })
                    .child(label),
            )
            .when_some(count, |item, count| {
                item.child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(theme.text_muted))
                        .child(count.to_string()),
                )
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.session_manager.selected_group = group.clone();
                    this.clear_session_selection_for_invisible_rows();
                    this.session_manager.show_batch_move = false;
                    this.session_manager.focused_input = None;
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn render_group_tree_node(
        &self,
        group: String,
        depth: usize,
        child_groups: &HashMap<String, Vec<String>>,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let children = child_groups.get(&group).cloned().unwrap_or_default();
        let has_children = !children.is_empty();
        let expanded = self.session_manager.expanded_groups.contains(&group);
        let active = self.session_manager.selected_group.as_deref() == Some(group.as_str());
        let label = group
            .rsplit('/')
            .next()
            .unwrap_or(group.as_str())
            .to_string();
        let selected_group = group.clone();
        let mut node = div().child(
            div()
                .min_h(px(28.0))
                .pl(px((depth.min(5) as f32 * 16.0) + 8.0))
                .pr_2()
                .flex()
                .items_center()
                .gap(px(4.0))
                .rounded(px(self.tokens.radii.md))
                .cursor_pointer()
                .bg(if active {
                    theme_active_bg(theme.bg_active, has_background)
                } else {
                    rgba(0x00000000)
                })
                .hover(move |item| {
                    if active {
                        item
                    } else {
                        item.bg(theme_hover_bg(theme.bg_hover, has_background))
                    }
                })
                .child(if has_children {
                    div()
                        .size(px(18.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(self.tokens.radii.md))
                        .hover(move |button| {
                            button.bg(theme_hover_bg(self.tokens.ui.bg_hover, has_background))
                        })
                        .child(Self::render_lucide_icon(
                            if expanded {
                                LucideIcon::ChevronDown
                            } else {
                                LucideIcon::ChevronRight
                            },
                            14.0,
                            rgb(theme.text_muted),
                        ))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener({
                                let group = group.clone();
                                move |this, _event, _window, cx| {
                                    this.toggle_session_group_expanded(&group);
                                    cx.notify();
                                    cx.stop_propagation();
                                }
                            }),
                        )
                        .into_any_element()
                } else {
                    div().w(px(18.0)).flex_none().into_any_element()
                })
                .child(Self::render_lucide_icon(
                    if expanded && has_children {
                        LucideIcon::FolderOpen
                    } else {
                        LucideIcon::Folder
                    },
                    16.0,
                    rgb(0xeab308),
                ))
                .child(
                    div()
                        .flex_1()
                        .truncate()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .font_weight(if active {
                            gpui::FontWeight::MEDIUM
                        } else {
                            gpui::FontWeight::NORMAL
                        })
                        .child(label),
                )
                .child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(theme.text_muted))
                        .child(self.connection_count_for_group(&group).to_string()),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.session_manager.selected_group = Some(selected_group.clone());
                        this.clear_session_selection_for_invisible_rows();
                        this.session_manager.show_batch_move = false;
                        this.session_manager.focused_input = None;
                        cx.notify();
                    }),
                ),
        );

        if expanded && has_children {
            for child in children {
                node = node.child(self.render_group_tree_node(
                    child,
                    depth + 1,
                    child_groups,
                    has_background,
                    cx,
                ));
            }
        }

        node.into_any_element()
    }

    fn render_new_group_tree_item(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .h(px(32.0))
            .w_full()
            .px_3()
            .flex()
            .items_center()
            .gap(px(8.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(theme_border(theme.border, has_background))
            .text_color(rgb(theme.text_muted))
            .cursor_pointer()
            .hover(move |item| {
                item.bg(theme_hover_bg(self.tokens.ui.bg_hover, has_background))
                    .text_color(rgb(self.tokens.ui.text))
            })
            .child(Self::render_lucide_icon(
                LucideIcon::Plus,
                16.0,
                rgb(theme.text_muted),
            ))
            .child(
                div()
                    .flex_1()
                    .truncate()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .child(self.i18n.t("sessionManager.folder_tree.new_group")),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.session_manager.show_new_group = true;
                    this.session_manager.focused_input = Some(SessionManagerInput::NewGroup);
                    this.session_manager.new_group_name.clear();
                    this.needs_active_pane_focus = false;
                    cx.notify();
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_session_manager_table(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let rows = self.filtered_session_connections();
        let all_selected = !rows.is_empty()
            && rows
                .iter()
                .all(|row| self.session_manager.selected_ids.contains(&row.id));
        let empty_message = if self.session_manager.search_query.trim().is_empty() {
            self.i18n.t("sessionManager.table.no_connections")
        } else {
            self.i18n.t("sessionManager.table.no_search_results")
        };
        div()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .flex()
            .flex_col()
            .bg(if has_background {
                rgba(0x00000000)
            } else {
                rgb(theme.bg)
            })
            .child(self.render_connection_table_header(has_background, all_selected, cx))
            .child(
                div()
                    .id("session-manager-table-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .when(rows.is_empty(), |body| {
                        body.flex().items_center().justify_center().child(
                            div()
                                .flex()
                                .flex_col()
                                .items_center()
                                .gap(px(16.0))
                                .py(px(64.0))
                                .text_color(rgb(theme.text_muted))
                                .child(Self::render_lucide_icon(
                                    LucideIcon::Server,
                                    48.0,
                                    rgba((theme.text_muted << 8) | 0x4d),
                                ))
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .items_center()
                                        .child(
                                            div()
                                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                                .font_weight(gpui::FontWeight::MEDIUM)
                                                .child(empty_message),
                                        )
                                        .child(
                                            div()
                                                .mt_1()
                                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                                .child(
                                                    self.i18n.t(
                                                        "sessionManager.table.no_connections_hint",
                                                    ),
                                                ),
                                        ),
                                )
                                .child(
                                    self.render_toolbar_button(
                                        LucideIcon::Plus,
                                        self.i18n.t("sessionManager.toolbar.new_connection"),
                                        ButtonVariant::Default,
                                        has_background,
                                        true,
                                    )
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _event, window, cx| {
                                            this.open_new_connection_form(window, cx);
                                            cx.stop_propagation();
                                        }),
                                    ),
                                ),
                        )
                    })
                    .children(
                        rows.into_iter()
                            .map(|conn| self.render_connection_table_row(conn, has_background, cx)),
                    ),
            )
            .into_any_element()
    }

    fn render_connection_table_header(
        &self,
        has_background: bool,
        all_selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .min_h(px(35.0))
            .flex()
            .items_center()
            .px_2()
            .py(px(6.0))
            .border_b_1()
            .border_color(theme_border(theme.border, has_background))
            .bg(theme_secondary_bg(theme.bg_secondary, has_background))
            .text_size(px(MANAGER_TABLE_HEADER_TEXT_SIZE))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(theme.text_muted))
            .child(
                div()
                    .w(px(MANAGER_COL_CHECKBOX))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        checkbox(&self.tokens, String::new(), all_selected).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.toggle_all_visible_connections(cx);
                                cx.stop_propagation();
                            }),
                        ),
                    ),
            )
            .child(self.render_sort_header(
                "sessionManager.table.name",
                SessionSortField::Name,
                MANAGER_COL_NAME_BASIS,
                true,
                cx,
            ))
            .child(self.render_sort_header(
                "sessionManager.table.host",
                SessionSortField::Host,
                MANAGER_COL_HOST,
                false,
                cx,
            ))
            .child(self.render_sort_header(
                "sessionManager.table.port",
                SessionSortField::Port,
                MANAGER_COL_PORT,
                false,
                cx,
            ))
            .child(self.render_sort_header(
                "sessionManager.table.username",
                SessionSortField::Username,
                MANAGER_COL_USERNAME,
                false,
                cx,
            ))
            .child(self.render_sort_header(
                "sessionManager.table.auth_type",
                SessionSortField::AuthType,
                MANAGER_COL_AUTH,
                false,
                cx,
            ))
            .child(self.render_sort_header(
                "sessionManager.table.group",
                SessionSortField::Group,
                MANAGER_COL_GROUP,
                false,
                cx,
            ))
            .child(self.render_sort_header(
                "sessionManager.table.last_used",
                SessionSortField::LastUsed,
                MANAGER_COL_LAST_USED,
                false,
                cx,
            ))
            .child(div().w(px(MANAGER_COL_ACTIONS)).flex_none())
            .into_any_element()
    }

    fn render_sort_header(
        &self,
        label_key: &'static str,
        field: SessionSortField,
        width: f32,
        flexible: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active = self.session_manager.sort_field == field;
        let (icon, icon_color) = if active {
            let icon = match self.session_manager.sort_direction {
                SortDirection::Asc => LucideIcon::ArrowUp,
                SortDirection::Desc => LucideIcon::ArrowDown,
            };
            (icon, rgb(0x60a5fa))
        } else {
            (
                LucideIcon::ArrowUpDown,
                rgba((theme.text_muted << 8) | 0x66),
            )
        };
        div()
            .when(flexible, |cell| {
                cell.flex_1().min_w(px(MANAGER_COL_NAME_MIN))
            })
            .when(!flexible, |cell| cell.w(px(width)).flex_none())
            .pl(if flexible { px(4.0) } else { px(0.0) })
            .flex()
            .items_center()
            .gap(px(4.0))
            .cursor_pointer()
            .hover(move |cell| cell.text_color(rgb(theme.text)))
            .child(div().truncate().child(self.i18n.t(label_key)))
            .child(Self::render_lucide_icon(icon, 14.0, icon_color))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if this.session_manager.sort_field == field {
                        this.session_manager.sort_direction =
                            this.session_manager.sort_direction.toggled();
                    } else {
                        this.session_manager.sort_field = field;
                        this.session_manager.sort_direction = SortDirection::Asc;
                    }
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn render_connection_table_row(
        &self,
        conn: ConnectionInfo,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let selected = self.session_manager.selected_ids.contains(&conn.id);
        let hovered =
            self.session_manager.hovered_connection_id.as_deref() == Some(conn.id.as_str());
        let id = conn.id.clone();
        let color = conn.color.as_deref().and_then(parse_hex_color);
        div()
            .id((
                gpui::ElementId::from("session-manager-table-row"),
                conn.id.clone(),
            ))
            .relative()
            .min_h(px(36.0))
            .flex()
            .items_center()
            .px_2()
            .py(px(6.0))
            .border_b_1()
            .border_color(theme_border_half(theme.border, has_background))
            .bg(if selected {
                rgba((theme.info << 8) | BG_ACTIVE_ROW_SELECTED_ALPHA)
            } else if has_background {
                rgba(0x00000000)
            } else {
                rgba(0x00000000)
            })
            .hover(move |row| row.bg(theme_hover_bg(theme.bg_hover, has_background)))
            .on_hover(cx.listener({
                let id = conn.id.clone();
                move |this, is_hovered: &bool, _window, cx| {
                    if *is_hovered {
                        this.session_manager.hovered_connection_id = Some(id.clone());
                    } else if this.session_manager.hovered_connection_id.as_deref()
                        == Some(id.as_str())
                    {
                        this.session_manager.hovered_connection_id = None;
                    }
                    cx.notify();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.session_manager.row_context_menu_connection_id = None;
                    if event.click_count == 2 {
                        this.open_saved_connection(&id, window, cx);
                    }
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener({
                    let id = conn.id.clone();
                    move |this, event: &MouseDownEvent, _window, cx| {
                        this.session_manager.row_context_menu_connection_id = Some(id.clone());
                        this.session_manager.row_context_menu_x = f32::from(event.position.x);
                        this.session_manager.row_context_menu_y = f32::from(event.position.y);
                        this.session_manager.row_menu_connection_id = None;
                        cx.notify();
                        cx.stop_propagation();
                    }
                }),
            )
            .when_some(color, |row, color| {
                row.child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .bottom_0()
                        .w(px(MANAGER_COLOR_INDICATOR_WIDTH))
                        .rounded_l(px(self.tokens.radii.sm))
                        .bg(rgb(color)),
                )
            })
            .child(
                div()
                    .w(px(MANAGER_COL_CHECKBOX))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        checkbox(&self.tokens, String::new(), selected).on_mouse_down(
                            MouseButton::Left,
                            cx.listener({
                                let id = conn.id.clone();
                                move |this, _event, _window, cx| {
                                    this.toggle_connection_selection(&id);
                                    cx.notify();
                                    cx.stop_propagation();
                                }
                            }),
                        ),
                    ),
            )
            .child(self.render_table_cell(
                conn.name.clone(),
                MANAGER_COL_NAME_BASIS,
                TableCellStyle::Primary,
                true,
            ))
            .child(self.render_table_cell(
                conn.host.clone(),
                MANAGER_COL_HOST,
                TableCellStyle::MetaMono,
                false,
            ))
            .child(self.render_table_cell(
                conn.port.to_string(),
                MANAGER_COL_PORT,
                TableCellStyle::MetaMono,
                false,
            ))
            .child(self.render_table_cell(
                conn.username.clone(),
                MANAGER_COL_USERNAME,
                TableCellStyle::Meta,
                false,
            ))
            .child(self.render_auth_badge_cell(conn.auth_type))
            .child(self.render_table_cell(
                conn.group.clone().unwrap_or_else(|| "—".to_string()),
                MANAGER_COL_GROUP,
                TableCellStyle::Meta,
                false,
            ))
            .child(self.render_table_cell(
                format_last_used(conn.last_used_at.as_deref(), &self.i18n),
                MANAGER_COL_LAST_USED,
                TableCellStyle::Meta,
                false,
            ))
            .child(div().w(px(MANAGER_COL_ACTIONS)).flex_none())
            .child(self.render_inline_row_actions(conn, hovered, has_background, cx))
            .into_any_element()
    }

    fn render_table_cell(
        &self,
        text: String,
        width: f32,
        style: TableCellStyle,
        flexible: bool,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let strong = style == TableCellStyle::Primary;
        let cell = div()
            .when(flexible, |cell| {
                cell.flex_1().min_w(px(MANAGER_COL_NAME_MIN))
            })
            .when(!flexible, |cell| cell.w(px(width)).flex_none())
            .pl(if flexible { px(4.0) } else { px(0.0) })
            .truncate()
            .text_size(px(match style {
                TableCellStyle::Primary => MANAGER_ROW_TEXT_SIZE,
                TableCellStyle::Meta | TableCellStyle::MetaMono => MANAGER_ROW_META_TEXT_SIZE,
            }))
            .font_weight(if strong {
                gpui::FontWeight::MEDIUM
            } else {
                gpui::FontWeight::NORMAL
            })
            .text_color(if strong {
                rgb(theme.text)
            } else {
                rgb(theme.text_muted)
            })
            .when(style == TableCellStyle::MetaMono, |cell| {
                cell.font_family(settings_mono_font_family(self.settings_store.settings()))
            });
        cell.child(text).into_any_element()
    }

    fn render_auth_badge_cell(&self, auth_type: AuthType) -> AnyElement {
        let theme = self.tokens.ui;
        let (icon, label, bg, fg) = auth_badge_style(auth_type, theme.text_muted, theme.text);
        div()
            .w(px(MANAGER_COL_AUTH))
            .flex_none()
            .flex()
            .items_center()
            .child(
                div()
                    .w(px(auth_badge_width(label)))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(MANAGER_AUTH_BADGE_GAP))
                    .px(px(MANAGER_AUTH_BADGE_PADDING_X))
                    .py(px(2.0))
                    .rounded(px(self.tokens.radii.md))
                    .bg(bg)
                    .text_color(fg)
                    .text_size(px(MANAGER_AUTH_BADGE_TEXT_SIZE))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child(Self::render_lucide_icon(
                        icon,
                        MANAGER_AUTH_BADGE_ICON_SIZE,
                        fg,
                    ))
                    .child(label),
            )
            .into_any_element()
    }

    fn render_inline_row_actions(
        &self,
        conn: ConnectionInfo,
        visible: bool,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let menu_open =
            self.session_manager.row_menu_connection_id.as_deref() == Some(conn.id.as_str());
        let actions_visible = visible || menu_open;
        div()
            .absolute()
            .right_0()
            .top_0()
            .bottom_0()
            .w(px(MANAGER_COL_ACTIONS))
            .flex()
            .items_center()
            .justify_end()
            .gap(px(2.0))
            .opacity(if actions_visible { 1.0 } else { 0.0 })
            .bg(if actions_visible {
                theme_hover_bg(theme.bg_hover, has_background)
            } else {
                theme_bg(theme.bg, has_background)
            })
            .child(
                self.render_row_icon_button(
                    LucideIcon::Play,
                    MANAGER_ROW_ACTION_BUTTON,
                    12.0,
                    rgb(0x4ade80),
                    has_background,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener({
                        let id = conn.id.clone();
                        move |this, _event, window, cx| {
                            this.session_manager.row_menu_connection_id = None;
                            this.open_saved_connection(&id, window, cx);
                            cx.stop_propagation();
                        }
                    }),
                ),
            )
            .child(
                self.render_row_icon_button(
                    LucideIcon::Pencil,
                    MANAGER_ROW_ACTION_BUTTON,
                    12.0,
                    rgb(theme.text),
                    has_background,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener({
                        let id = conn.id.clone();
                        move |this, _event, window, cx| {
                            this.session_manager.row_menu_connection_id = None;
                            this.open_saved_connection_editor(&id, None, window, cx);
                            cx.stop_propagation();
                        }
                    }),
                ),
            )
            .child(
                self.render_row_icon_button(
                    LucideIcon::MoreHorizontal,
                    MANAGER_ROW_MORE_BUTTON,
                    14.0,
                    rgb(theme.text),
                    has_background,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener({
                        let id = conn.id.clone();
                        move |this, event: &MouseDownEvent, window, cx| {
                            this.session_manager.row_menu_connection_id =
                                if this.session_manager.row_menu_connection_id.as_deref()
                                    == Some(id.as_str())
                                {
                                    None
                                } else {
                                    Some(id.clone())
                                };
                            let viewport_height = f32::from(window.viewport_size().height);
                            this.session_manager.row_menu_opens_above = f32::from(event.position.y)
                                + MANAGER_ROW_MENU_HEIGHT
                                > viewport_height;
                            cx.notify();
                            cx.stop_propagation();
                        }
                    }),
                ),
            )
            .when(menu_open, |actions| {
                actions.child(self.render_row_more_menu(
                    conn,
                    has_background,
                    self.session_manager.row_menu_opens_above,
                    cx,
                ))
            })
            .into_any_element()
    }

    fn render_row_more_menu(
        &self,
        conn: ConnectionInfo,
        has_background: bool,
        opens_above: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .absolute()
            .when(!opens_above, |menu| menu.top(px(30.0)))
            .when(opens_above, |menu| menu.bottom(px(30.0)))
            .right(px(0.0))
            .w(px(MANAGER_ROW_MENU_WIDTH))
            .p(px(4.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(theme_border(theme.border, has_background))
            .bg(theme_panel_bg(theme.bg_panel, has_background))
            .shadow_lg()
            .child(
                self.render_row_menu_item(
                    LucideIcon::Zap,
                    self.i18n.t("sessionManager.actions.test_connection"),
                    rgb(theme.text),
                    has_background,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener({
                        let id = conn.id.clone();
                        move |this, _event, window, cx| {
                            this.session_manager.row_menu_connection_id = None;
                            this.test_connection(&id, window, cx);
                            cx.stop_propagation();
                        }
                    }),
                ),
            )
            .child(
                self.render_row_menu_item(
                    LucideIcon::Copy,
                    self.i18n.t("sessionManager.actions.duplicate"),
                    rgb(theme.text),
                    has_background,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener({
                        let id = conn.id.clone();
                        move |this, _event, _window, cx| {
                            this.session_manager.row_menu_connection_id = None;
                            this.duplicate_connection(&id, cx);
                            cx.stop_propagation();
                        }
                    }),
                ),
            )
            .child(
                div()
                    .h(px(1.0))
                    .my(px(4.0))
                    .bg(theme_border_half(theme.border, has_background)),
            )
            .child(
                self.render_row_menu_item(
                    LucideIcon::Trash2,
                    self.i18n.t("sessionManager.actions.delete"),
                    rgb(0xf87171),
                    has_background,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener({
                        let id = conn.id.clone();
                        move |this, _event, _window, cx| {
                            this.session_manager.row_menu_connection_id = None;
                            this.delete_connection(&id, cx);
                            cx.stop_propagation();
                        }
                    }),
                ),
            )
            .into_any_element()
    }

    fn render_row_context_menu(
        &self,
        conn: ConnectionInfo,
        window: &Window,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let viewport = window.viewport_size();
        let x = self
            .session_manager
            .row_context_menu_x
            .min(f32::from(viewport.width) - MANAGER_ROW_MENU_WIDTH - 8.0)
            .max(8.0);
        let y = self
            .session_manager
            .row_context_menu_y
            .min(f32::from(viewport.height) - MANAGER_ROW_CONTEXT_MENU_HEIGHT - 8.0)
            .max(8.0);
        let theme = self.tokens.ui;
        div()
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(MANAGER_ROW_MENU_WIDTH))
            .p(px(4.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(theme_border(theme.border, has_background))
            .bg(theme_panel_bg(theme.bg_panel, has_background))
            .shadow_lg()
            .child(
                self.render_row_menu_item_with_icon_color(
                    LucideIcon::Play,
                    self.i18n.t("sessionManager.actions.connect"),
                    rgb(theme.text),
                    rgb(0x4ade80),
                    has_background,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener({
                        let id = conn.id.clone();
                        move |this, _event, window, cx| {
                            this.close_session_row_menus();
                            this.open_saved_connection(&id, window, cx);
                            cx.stop_propagation();
                        }
                    }),
                ),
            )
            .child(
                self.render_row_menu_item_with_icon_color(
                    LucideIcon::Zap,
                    self.i18n.t("sessionManager.actions.test_connection"),
                    rgb(theme.text),
                    rgb(theme.text),
                    has_background,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener({
                        let id = conn.id.clone();
                        move |this, _event, window, cx| {
                            this.close_session_row_menus();
                            this.test_connection(&id, window, cx);
                            cx.stop_propagation();
                        }
                    }),
                ),
            )
            .child(
                self.render_row_menu_item_with_icon_color(
                    LucideIcon::Pencil,
                    self.i18n.t("sessionManager.actions.edit"),
                    rgb(theme.text),
                    rgb(theme.text),
                    has_background,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener({
                        let id = conn.id.clone();
                        move |this, _event, window, cx| {
                            this.close_session_row_menus();
                            this.open_saved_connection_editor(&id, None, window, cx);
                            cx.stop_propagation();
                        }
                    }),
                ),
            )
            .child(
                self.render_row_menu_item_with_icon_color(
                    LucideIcon::Copy,
                    self.i18n.t("sessionManager.actions.duplicate"),
                    rgb(theme.text),
                    rgb(theme.text),
                    has_background,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener({
                        let id = conn.id.clone();
                        move |this, _event, _window, cx| {
                            this.close_session_row_menus();
                            this.duplicate_connection(&id, cx);
                            cx.stop_propagation();
                        }
                    }),
                ),
            )
            .child(
                div()
                    .h(px(1.0))
                    .my(px(4.0))
                    .bg(theme_border_half(theme.border, has_background)),
            )
            .child(
                self.render_row_menu_item_with_icon_color(
                    LucideIcon::Trash2,
                    self.i18n.t("sessionManager.actions.delete"),
                    rgb(0xf87171),
                    rgb(0xf87171),
                    has_background,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener({
                        let id = conn.id.clone();
                        move |this, _event, _window, cx| {
                            this.close_session_row_menus();
                            this.delete_connection(&id, cx);
                            cx.stop_propagation();
                        }
                    }),
                ),
            )
            .into_any_element()
    }

    fn render_row_menu_item(
        &self,
        icon: LucideIcon,
        label: String,
        color: Rgba,
        has_background: bool,
    ) -> gpui::Div {
        div()
            .h(px(30.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .px_2()
            .rounded(px(self.tokens.radii.sm))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(color)
            .cursor_pointer()
            .hover(move |item| item.bg(theme_hover_bg(self.tokens.ui.bg_hover, has_background)))
            .child(Self::render_lucide_icon(icon, 16.0, color))
            .child(label)
    }

    fn render_row_menu_item_with_icon_color(
        &self,
        icon: LucideIcon,
        label: String,
        text_color: Rgba,
        icon_color: Rgba,
        has_background: bool,
    ) -> gpui::Div {
        div()
            .h(px(30.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .px_2()
            .rounded(px(self.tokens.radii.sm))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(text_color)
            .cursor_pointer()
            .hover(move |item| item.bg(theme_hover_bg(self.tokens.ui.bg_hover, has_background)))
            .child(Self::render_lucide_icon(icon, 16.0, icon_color))
            .child(label)
    }

    fn render_row_icon_button(
        &self,
        icon: LucideIcon,
        size: f32,
        icon_size: f32,
        icon_color: Rgba,
        has_background: bool,
    ) -> gpui::Div {
        div()
            .size(px(size))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(self.tokens.radii.sm))
            .cursor_pointer()
            .hover(move |button| button.bg(theme_hover_bg(self.tokens.ui.bg_hover, has_background)))
            .child(Self::render_lucide_icon(icon, icon_size, icon_color))
    }

    #[allow(dead_code)]
    fn render_row_actions(&self, conn: ConnectionInfo, cx: &mut Context<Self>) -> AnyElement {
        div()
            .w(px(210.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .child(
                self.render_row_action_button(
                    self.i18n.t("sessionManager.actions.connect"),
                    ButtonVariant::Default,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener({
                        let id = conn.id.clone();
                        move |this, _event, window, cx| {
                            this.open_saved_connection(&id, window, cx);
                            cx.stop_propagation();
                        }
                    }),
                ),
            )
            .child(
                self.render_row_action_button(
                    self.i18n.t("sessionManager.actions.edit"),
                    ButtonVariant::Secondary,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener({
                        let id = conn.id.clone();
                        move |this, _event, window, cx| {
                            this.open_saved_connection_editor(&id, None, window, cx);
                            cx.stop_propagation();
                        }
                    }),
                ),
            )
            .child(
                self.render_row_action_button(
                    self.i18n.t("sessionManager.actions.duplicate"),
                    ButtonVariant::Secondary,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener({
                        let id = conn.id.clone();
                        move |this, _event, _window, cx| {
                            this.duplicate_connection(&id, cx);
                            cx.stop_propagation();
                        }
                    }),
                ),
            )
            .child(
                self.render_row_action_button(
                    self.i18n.t("sessionManager.actions.delete"),
                    ButtonVariant::Destructive,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener({
                        let id = conn.id.clone();
                        move |this, _event, _window, cx| {
                            this.delete_connection(&id, cx);
                            cx.stop_propagation();
                        }
                    }),
                ),
            )
            .into_any_element()
    }

    #[allow(dead_code)]
    fn render_row_action_button(&self, label: String, variant: ButtonVariant) -> gpui::Div {
        button_with(
            &self.tokens,
            label,
            ButtonOptions {
                variant,
                size: ButtonSize::Sm,
                radius: ButtonRadius::Md,
                disabled: false,
            },
        )
    }

    fn render_session_manager_button(
        &self,
        icon: LucideIcon,
        label: String,
        variant: ButtonVariant,
    ) -> gpui::Div {
        let theme = self.tokens.ui;
        button_with(
            &self.tokens,
            label,
            ButtonOptions {
                variant,
                size: ButtonSize::Sm,
                radius: ButtonRadius::Md,
                disabled: false,
            },
        )
        .gap(px(6.0))
        .child(Self::render_lucide_icon(icon, 14.0, rgb(theme.text)))
    }

    fn render_toolbar_button(
        &self,
        icon: LucideIcon,
        label: String,
        variant: ButtonVariant,
        has_background: bool,
        show_label: bool,
    ) -> gpui::Div {
        let theme = self.tokens.ui;
        let (bg, border, text) = match variant {
            ButtonVariant::Default => (rgb(theme.text), rgba(0x00000000), rgb(theme.bg)),
            ButtonVariant::Outline => (
                rgba(0x00000000),
                theme_border(theme.border, has_background),
                rgb(theme.text),
            ),
            _ => (
                theme_panel_bg(theme.bg_panel, has_background),
                theme_border(theme.border, has_background),
                rgb(theme.text),
            ),
        };
        div()
            .h(px(32.0))
            .px_3()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(6.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(border)
            .bg(bg)
            .text_color(text)
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .font_weight(gpui::FontWeight::MEDIUM)
            .cursor_pointer()
            .child(Self::render_lucide_icon(icon, 16.0, text))
            .when(show_label, |button| button.child(label))
    }

    fn render_toolbar_link_icon(
        &self,
        icon: LucideIcon,
        label_key: &str,
        opens_import: bool,
        show_label: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = self.i18n.t(label_key);
        div()
            .h(px(32.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(self.tokens.ui.text))
            .cursor_pointer()
            .child(Self::render_lucide_icon(
                icon,
                16.0,
                rgb(self.tokens.ui.text),
            ))
            .when(show_label, |button| button.child(label))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if opens_import {
                        this.open_ssh_config_import(cx);
                    }
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    pub(super) fn render_session_text_input(
        &self,
        target: SessionManagerInput,
        value: &str,
        placeholder: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let workspace = cx.entity();
        let active = self.session_manager.focused_input == Some(target);
        let has_background = self
            .terminal_background_preferences("session_manager")
            .is_some();
        let text = if value.is_empty() {
            placeholder
        } else {
            value.to_string()
        };
        let input_target = WorkspaceImeTarget::SessionManager(target);
        text_input_anchor_probe(
            input_target.anchor_id(),
            div()
                .h(px(32.0))
                .w_full()
                .px_3()
                .flex()
                .items_center()
                .gap(px(8.0))
                .rounded(px(self.tokens.radii.md))
                .border_1()
                .border_color(if active {
                    rgb(theme.accent)
                } else {
                    theme_border_half(theme.border, has_background)
                })
                .bg(theme_input_bg(theme.bg, has_background))
                .text_size(px(self.tokens.metrics.ui_text_sm))
                .text_color(if value.is_empty() {
                    rgb(theme.text_muted)
                } else {
                    rgb(theme.text)
                })
                .when(
                    matches!(
                        target,
                        SessionManagerInput::Search | SessionManagerInput::SavedSearch
                    ),
                    |input| {
                        input.child(Self::render_lucide_icon(
                            LucideIcon::Search,
                            16.0,
                            rgb(theme.text_muted),
                        ))
                    },
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .flex()
                        .items_center()
                        .overflow_hidden()
                        .when(active && value.is_empty(), |input| {
                            input.child(text_caret(&self.tokens, self.new_connection_caret_visible))
                        })
                        .child(div().truncate().child(text))
                        .when_some(
                            self.marked_text_for_target(input_target),
                            |input, marked| {
                                input.child(
                                    div()
                                        .underline()
                                        .text_color(rgb(theme.text))
                                        .child(marked.to_string()),
                                )
                            },
                        )
                        .when(active && !value.is_empty(), |input| {
                            input.child(text_caret(&self.tokens, self.new_connection_caret_visible))
                        }),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, window, cx| {
                        this.session_manager.focused_input = Some(target);
                        this.ime_marked_text = None;
                        this.needs_active_pane_focus = false;
                        window.focus(&this.focus_handle);
                        cx.notify();
                        cx.stop_propagation();
                    }),
                ),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    fn render_new_group_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
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
            .bg(rgba(0x00000066))
            .child(
                div()
                    .w(px(380.0))
                    .flex()
                    .flex_col()
                    .gap(px(14.0))
                    .p(px(16.0))
                    .rounded(px(self.tokens.radii.lg))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .bg(rgb(theme.bg_panel))
                    .child(
                        div()
                            .text_size(px(18.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(self.i18n.t("sessionManager.folder_tree.new_group")),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(theme.text_muted))
                            .child(
                                self.i18n
                                    .t("sessionManager.folder_tree.new_group_description"),
                            ),
                    )
                    .child(
                        self.render_session_text_input(
                            SessionManagerInput::NewGroup,
                            &self.session_manager.new_group_name,
                            self.i18n
                                .t("sessionManager.folder_tree.new_group_placeholder"),
                            cx,
                        ),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.0))
                            .child(
                                button_with(
                                    &self.tokens,
                                    self.i18n.t("sessionManager.edit_properties.cancel"),
                                    ButtonOptions {
                                        variant: ButtonVariant::Secondary,
                                        size: ButtonSize::Sm,
                                        radius: ButtonRadius::Md,
                                        disabled: false,
                                    },
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.session_manager.show_new_group = false;
                                        this.session_manager.focused_input = None;
                                        cx.notify();
                                    }),
                                ),
                            )
                            .child(
                                button_with(
                                    &self.tokens,
                                    self.i18n.t("sessionManager.edit_properties.save"),
                                    ButtonOptions {
                                        variant: ButtonVariant::Default,
                                        size: ButtonSize::Sm,
                                        radius: ButtonRadius::Md,
                                        disabled: false,
                                    },
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.create_session_group(cx);
                                    }),
                                ),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_ssh_config_import_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
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
            .bg(rgba(0x00000066))
            .child(
                div()
                    .w(px(620.0))
                    .max_h(px(520.0))
                    .flex()
                    .flex_col()
                    .rounded(px(self.tokens.radii.lg))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .bg(rgb(theme.bg_panel))
                    .child(
                        div()
                            .h(px(48.0))
                            .flex()
                            .items_center()
                            .px_4()
                            .border_b_1()
                            .border_color(rgb(theme.border))
                            .text_size(px(18.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("SSH Config"),
                    )
                    .child(
                        div()
                            .id("session-manager-import-scroll")
                            .flex_1()
                            .min_h(px(0.0))
                            .overflow_y_scroll()
                            .children(
                                self.session_manager
                                    .ssh_config_hosts
                                    .iter()
                                    .cloned()
                                    .map(|host| self.render_import_host_row(host, cx)),
                            ),
                    )
                    .child(
                        div()
                            .h(px(54.0))
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap(px(8.0))
                            .px_4()
                            .border_t_1()
                            .border_color(rgb(theme.border))
                            .child(
                                button_with(
                                    &self.tokens,
                                    self.i18n.t("sessionManager.edit_properties.cancel"),
                                    ButtonOptions {
                                        variant: ButtonVariant::Secondary,
                                        size: ButtonSize::Sm,
                                        radius: ButtonRadius::Md,
                                        disabled: false,
                                    },
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.session_manager.show_import = false;
                                        this.session_manager.selected_import_aliases.clear();
                                        cx.notify();
                                    }),
                                ),
                            )
                            .child(
                                button_with(
                                    &self.tokens,
                                    self.i18n.t("sessionManager.toolbar.import"),
                                    ButtonOptions {
                                        variant: ButtonVariant::Default,
                                        size: ButtonSize::Sm,
                                        radius: ButtonRadius::Md,
                                        disabled: false,
                                    },
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.import_selected_ssh_hosts(cx);
                                    }),
                                ),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_import_host_row(&self, host: SshConfigHost, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let checked = self
            .session_manager
            .selected_import_aliases
            .contains(&host.alias);
        let alias = host.alias.clone();
        div()
            .h(px(44.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .px_4()
            .border_b_1()
            .border_color(rgba((theme.border << 8) | 0x80))
            .child(
                checkbox(&self.tokens, String::new(), checked).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        if this
                            .session_manager
                            .selected_import_aliases
                            .contains(&alias)
                        {
                            this.session_manager.selected_import_aliases.remove(&alias);
                        } else {
                            this.session_manager
                                .selected_import_aliases
                                .insert(alias.clone());
                        }
                        cx.notify();
                        cx.stop_propagation();
                    }),
                ),
            )
            .child(
                div()
                    .w(px(150.0))
                    .truncate()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(host.alias),
            )
            .child(
                div()
                    .flex_1()
                    .truncate()
                    .text_color(rgb(theme.text_muted))
                    .child(format!(
                        "{}@{}:{}",
                        host.user.unwrap_or_else(|| current_username()),
                        host.hostname.unwrap_or_else(|| "-".to_string()),
                        host.port.unwrap_or(22)
                    )),
            )
            .when(host.already_imported, |row| {
                row.child(
                    div()
                        .px_2()
                        .py(px(2.0))
                        .rounded(px(self.tokens.radii.md))
                        .bg(rgba((theme.success << 8) | 0x2a))
                        .text_color(rgb(theme.success))
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .child("Imported"),
                )
            })
            .into_any_element()
    }

    fn render_batch_move_popover(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let groups = self.connection_store.groups().to_vec();
        div()
            .id("session-manager-batch-move-scroll")
            .absolute()
            .top(px(44.0))
            .right(px(104.0))
            .w(px(220.0))
            .max_h(px(260.0))
            .overflow_y_scroll()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg_panel))
            .shadow_lg()
            .child(self.render_batch_move_item(
                None,
                self.i18n.t("sessionManager.folder_tree.ungrouped"),
                cx,
            ))
            .children(
                groups
                    .into_iter()
                    .map(|group| self.render_batch_move_item(Some(group.clone()), group, cx)),
            )
            .into_any_element()
    }

    fn render_batch_move_item(
        &self,
        group: Option<String>,
        label: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .h(px(34.0))
            .px_3()
            .flex()
            .items_center()
            .cursor_pointer()
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .hover(move |row| row.bg(rgb(theme.bg_hover)))
            .child(label)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.move_selected_connections(group.as_deref(), cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn filtered_session_connections(&self) -> Vec<ConnectionInfo> {
        let query = self.session_manager.search_query.trim().to_lowercase();
        let mut rows = self.connection_store.connection_infos();
        rows.retain(|conn| self.connection_matches_filter(conn));
        if !query.is_empty() {
            rows.retain(|conn| {
                conn.name.to_lowercase().contains(&query)
                    || conn.host.to_lowercase().contains(&query)
                    || conn.username.to_lowercase().contains(&query)
                    || conn
                        .group
                        .as_deref()
                        .unwrap_or_default()
                        .to_lowercase()
                        .contains(&query)
                    || conn
                        .tags
                        .iter()
                        .any(|tag| tag.to_lowercase().contains(&query))
            });
        }
        self.sort_session_rows(&mut rows);
        if self.session_manager.selected_group.as_deref() == Some(RECENT_FILTER) {
            rows.truncate(20);
        }
        rows
    }

    fn connection_matches_filter(&self, conn: &ConnectionInfo) -> bool {
        match self.session_manager.selected_group.as_deref() {
            None => true,
            Some(UNGROUPED_FILTER) => conn.group.is_none(),
            Some(RECENT_FILTER) => conn.last_used_at.is_some(),
            Some(group) => conn.group.as_deref().is_some_and(|conn_group| {
                conn_group == group || conn_group.starts_with(&format!("{group}/"))
            }),
        }
    }

    fn sort_session_rows(&self, rows: &mut [ConnectionInfo]) {
        let field = self.session_manager.sort_field;
        rows.sort_by(|left, right| {
            let ordering = match field {
                SessionSortField::Name => left.name.to_lowercase().cmp(&right.name.to_lowercase()),
                SessionSortField::Host => left.host.to_lowercase().cmp(&right.host.to_lowercase()),
                SessionSortField::Port => left.port.cmp(&right.port),
                SessionSortField::Username => left
                    .username
                    .to_lowercase()
                    .cmp(&right.username.to_lowercase()),
                SessionSortField::AuthType => {
                    auth_label(left.auth_type).cmp(&auth_label(right.auth_type))
                }
                SessionSortField::Group => left.group.cmp(&right.group),
                SessionSortField::LastUsed => left.last_used_at.cmp(&right.last_used_at),
            };
            match self.session_manager.sort_direction {
                SortDirection::Asc => ordering,
                SortDirection::Desc => ordering.reverse(),
            }
        });
    }

    fn connection_count_for_group(&self, group: &str) -> usize {
        self.connection_store
            .connections()
            .iter()
            .filter(|conn| {
                conn.group.as_deref().is_some_and(|candidate| {
                    candidate == group || candidate.starts_with(&format!("{group}/"))
                })
            })
            .count()
    }

    fn session_group_tree(&self) -> (Vec<String>, HashMap<String, Vec<String>>) {
        let mut paths = HashSet::new();
        for group in self.connection_store.groups() {
            add_group_path_segments(group, &mut paths);
        }
        for conn in self.connection_store.connections() {
            if let Some(group) = conn.group.as_deref() {
                add_group_path_segments(group, &mut paths);
            }
        }

        let mut sorted = paths.into_iter().collect::<Vec<_>>();
        sorted.sort();
        let mut roots = Vec::new();
        let mut children: HashMap<String, Vec<String>> = HashMap::new();
        for path in sorted {
            if let Some((parent, _name)) = path.rsplit_once('/') {
                children.entry(parent.to_string()).or_default().push(path);
            } else {
                roots.push(path);
            }
        }
        (roots, children)
    }

    fn toggle_session_group_expanded(&mut self, group: &str) {
        if self.session_manager.expanded_groups.contains(group) {
            self.session_manager.expanded_groups.remove(group);
        } else {
            self.session_manager
                .expanded_groups
                .insert(group.to_string());
        }
    }

    fn connection_info_by_id(&self, id: &str) -> Option<ConnectionInfo> {
        self.connection_store
            .connection_infos()
            .into_iter()
            .find(|conn| conn.id == id)
    }

    fn close_session_row_menus(&mut self) {
        self.session_manager.row_menu_connection_id = None;
        self.session_manager.row_context_menu_connection_id = None;
    }

    fn toggle_connection_selection(&mut self, id: &str) {
        if self.session_manager.selected_ids.contains(id) {
            self.session_manager.selected_ids.remove(id);
        } else {
            self.session_manager.selected_ids.insert(id.to_string());
        }
    }

    fn toggle_all_visible_connections(&mut self, cx: &mut Context<Self>) {
        let rows = self.filtered_session_connections();
        let all_selected = !rows.is_empty()
            && rows
                .iter()
                .all(|row| self.session_manager.selected_ids.contains(&row.id));
        if all_selected {
            for row in rows {
                self.session_manager.selected_ids.remove(&row.id);
            }
        } else {
            for row in rows {
                self.session_manager.selected_ids.insert(row.id);
            }
        }
        cx.notify();
    }

    pub(super) fn clear_session_selection_for_invisible_rows(&mut self) {
        let visible_ids = self
            .filtered_session_connections()
            .into_iter()
            .map(|conn| conn.id)
            .collect::<HashSet<_>>();
        self.session_manager
            .selected_ids
            .retain(|id| visible_ids.contains(id));
    }

    fn create_session_group(&mut self, cx: &mut Context<Self>) {
        let name = self.session_manager.new_group_name.trim().to_string();
        match self.connection_store.create_group(name.clone()) {
            Ok(()) => {
                self.session_manager.selected_group = Some(name);
                expand_group_path(
                    self.session_manager
                        .selected_group
                        .as_deref()
                        .unwrap_or_default(),
                    &mut self.session_manager.expanded_groups,
                );
                self.session_manager.show_new_group = false;
                self.session_manager.focused_input = None;
                self.session_manager.status =
                    Some(self.i18n.t("sessionManager.toast.group_created"));
            }
            Err(error) => {
                self.session_manager.status = Some(format!(
                    "{}: {error}",
                    self.i18n.t("sessionManager.toast.create_group_failed")
                ));
            }
        }
        cx.notify();
    }

    #[allow(dead_code)]
    fn delete_connection(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Err(error) = self.connection_store.delete(id) {
            self.session_manager.status = Some(error.to_string());
        } else {
            self.session_manager.selected_ids.remove(id);
            self.session_manager.status =
                Some(self.i18n.t("sessionManager.toast.connection_deleted"));
        }
        cx.notify();
    }

    fn delete_selected_connections(&mut self, cx: &mut Context<Self>) {
        let ids = self
            .session_manager
            .selected_ids
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        let mut deleted = 0;
        for id in ids {
            if self.connection_store.delete(&id).unwrap_or(false) {
                deleted += 1;
            }
        }
        self.session_manager.selected_ids.clear();
        self.session_manager.show_batch_move = false;
        self.session_manager.status = Some(connections_deleted_label(&self.i18n, deleted));
        cx.notify();
    }

    #[allow(dead_code)]
    fn duplicate_connection(&mut self, id: &str, cx: &mut Context<Self>) {
        match self.connection_store.duplicate(id) {
            Ok(Some(_)) => {
                self.session_manager.status =
                    Some(self.i18n.t("sessionManager.toast.connection_duplicated"));
            }
            Ok(None) => {}
            Err(error) => self.session_manager.status = Some(error.to_string()),
        }
        cx.notify();
    }

    fn test_connection(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(conn) = self.connection_store.get(id).cloned() else {
            self.session_manager.status = Some(self.i18n.t("sessionManager.toast.test_failed"));
            cx.notify();
            return;
        };
        let loaded_password = match &conn.auth {
            SavedAuth::Password {
                keychain_id: Some(_),
                ..
            } => self.connection_store.get_connection_password(id).ok(),
            _ => None,
        };
        let loaded_passphrase = self
            .connection_store
            .get_connection_passphrase(id)
            .ok()
            .flatten();
        let Some(config) =
            ssh_config_from_saved_connection(&conn, loaded_password, loaded_passphrase)
        else {
            self.open_saved_connection_prompt(
                id,
                SavedConnectionPromptAction::Test,
                Some(
                    self.i18n
                        .t("sessionManager.edit_properties.password_placeholder"),
                ),
                window,
                cx,
            );
            return;
        };
        self.session_manager.status = Some(self.i18n.t("ssh.form.test_running"));
        self.start_ssh_test(config, cx);
    }

    fn move_selected_connections(&mut self, group: Option<&str>, cx: &mut Context<Self>) {
        let ids = self
            .session_manager
            .selected_ids
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        match self.connection_store.move_to_group(&ids, group) {
            Ok(count) => {
                self.session_manager.status = Some(connections_moved_label(
                    &self.i18n,
                    count,
                    group_label(&self.i18n, group),
                ));
                self.session_manager.selected_ids.clear();
                self.session_manager.show_batch_move = false;
            }
            Err(error) => self.session_manager.status = Some(error.to_string()),
        }
        cx.notify();
    }

    fn open_ssh_config_import(&mut self, cx: &mut Context<Self>) {
        let names = self
            .connection_store
            .connections()
            .iter()
            .map(|conn| conn.name.clone())
            .collect::<HashSet<_>>();
        match list_ssh_config_hosts(&names) {
            Ok(hosts) => {
                self.session_manager.selected_import_aliases = hosts
                    .iter()
                    .filter(|host| !host.already_imported)
                    .map(|host| host.alias.clone())
                    .collect();
                self.session_manager.ssh_config_hosts = hosts;
                self.session_manager.show_import = true;
                self.session_manager.status = None;
            }
            Err(error) => self.session_manager.status = Some(error.to_string()),
        }
        cx.notify();
    }

    fn import_selected_ssh_hosts(&mut self, cx: &mut Context<Self>) {
        let aliases = self
            .session_manager
            .selected_import_aliases
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        let mut imported = 0;
        let mut errors = Vec::new();
        for alias in aliases {
            match resolve_ssh_config_alias(&alias) {
                Ok(Some(host)) => match saved_connection_from_ssh_host(host) {
                    Ok(connection) => {
                        if self
                            .connection_store
                            .import_ssh_connection(connection)
                            .is_ok()
                        {
                            imported += 1;
                        }
                    }
                    Err(error) => errors.push(format!("{alias}: {error}")),
                },
                Ok(None) => errors.push(alias),
                Err(error) => errors.push(format!("{alias}: {error}")),
            }
        }
        self.session_manager.show_import = false;
        self.session_manager.selected_import_aliases.clear();
        self.session_manager.status = if errors.is_empty() {
            Some(format!("Imported {imported}"))
        } else {
            Some(format!("Imported {imported}; {}", errors.join(", ")))
        };
        cx.notify();
    }
}

fn auth_label(auth_type: AuthType) -> String {
    match auth_type {
        AuthType::Password => "Password",
        AuthType::Key => "Key",
        AuthType::Certificate => "Certificate",
        AuthType::Agent => "Agent",
    }
    .to_string()
}

fn add_group_path_segments(group: &str, paths: &mut HashSet<String>) {
    if group.trim().is_empty() {
        return;
    }
    let parts = group
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    for index in 1..=parts.len() {
        paths.insert(parts[..index].join("/"));
    }
}

fn expand_group_path(group: &str, expanded_groups: &mut HashSet<String>) {
    let parts = group
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() <= 1 {
        return;
    }
    for index in 1..parts.len() {
        expanded_groups.insert(parts[..index].join("/"));
    }
}

fn auth_badge_style(
    auth_type: AuthType,
    text_muted: u32,
    text: u32,
) -> (LucideIcon, &'static str, Rgba, Rgba) {
    match auth_type {
        AuthType::Key => (LucideIcon::Key, "Key", rgba(0x10b98133), rgb(0x6ee7b7)),
        AuthType::Password => (LucideIcon::Lock, "Pwd", rgba(0xf59e0b33), rgb(0xfcd34d)),
        AuthType::Agent => (LucideIcon::Bot, "Agent", rgba(0x3b82f633), rgb(0x93c5fd)),
        AuthType::Certificate => (
            LucideIcon::ShieldQuestion,
            "certificate",
            rgba((text_muted << 8) | 0x33),
            rgb(text),
        ),
    }
}

fn auth_badge_width(label: &str) -> f32 {
    MANAGER_AUTH_BADGE_PADDING_X * 2.0
        + MANAGER_AUTH_BADGE_ICON_SIZE
        + MANAGER_AUTH_BADGE_GAP
        + label.chars().count() as f32 * MANAGER_AUTH_BADGE_CHAR_WIDTH
}

fn format_last_used(last_used: Option<&str>, i18n: &I18n) -> String {
    let Some(last_used) = last_used else {
        return i18n.t("sessionManager.table.never_used");
    };
    let Ok(date) = DateTime::parse_from_rfc3339(last_used) else {
        return last_used.to_string();
    };
    let date = date.with_timezone(&Utc);
    let now = Utc::now();
    let diff = now.signed_duration_since(date);
    let diff_mins = diff.num_minutes();
    let diff_hours = diff.num_hours();
    let diff_days = diff.num_days();

    if diff_mins < 1 {
        return i18n.t("sessionManager.time.just_now");
    }
    if diff_mins < 60 {
        return i18n
            .t("sessionManager.time.minutes_ago")
            .replace("{{count}}", &diff_mins.to_string());
    }
    if diff_hours < 24 {
        return i18n
            .t("sessionManager.time.hours_ago")
            .replace("{{count}}", &diff_hours.to_string());
    }
    if diff_days < 7 {
        return i18n
            .t("sessionManager.time.days_ago")
            .replace("{{count}}", &diff_days.to_string());
    }

    let local = date.with_timezone(&Local);
    format!("{}/{}/{}", local.year(), local.month(), local.day())
}

fn theme_bg(color: u32, has_background: bool) -> Rgba {
    if has_background {
        rgba((color << 8) | BG_ACTIVE_THEME_ALPHA)
    } else {
        rgb(color)
    }
}

fn theme_panel_bg(color: u32, has_background: bool) -> Rgba {
    theme_bg(color, has_background)
}

fn theme_secondary_bg(color: u32, has_background: bool) -> Rgba {
    theme_bg(color, has_background)
}

fn theme_active_bg(color: u32, has_background: bool) -> Rgba {
    if has_background {
        rgba((color << 8) | BG_ACTIVE_THEME_ALPHA)
    } else {
        rgb(color)
    }
}

fn theme_hover_bg(color: u32, has_background: bool) -> Rgba {
    if has_background {
        rgba((color << 8) | BG_ACTIVE_HOVER_ALPHA)
    } else {
        rgb(color)
    }
}

fn theme_input_bg(color: u32, has_background: bool) -> Rgba {
    if has_background {
        rgba((color << 8) | (BG_ACTIVE_THEME_ALPHA / 2))
    } else {
        rgba((color << 8) | 0x80)
    }
}

fn theme_border(color: u32, has_background: bool) -> Rgba {
    if has_background {
        rgba((color << 8) | BG_ACTIVE_BORDER_ALPHA)
    } else {
        rgb(color)
    }
}

fn theme_border_half(color: u32, has_background: bool) -> Rgba {
    if has_background {
        rgba((color << 8) | BG_ACTIVE_BORDER_HALF_ALPHA)
    } else {
        rgba((color << 8) | 0x80)
    }
}

fn parse_hex_color(value: &str) -> Option<u32> {
    let hex = value.trim().strip_prefix('#')?;
    let expanded;
    let hex = match hex.len() {
        3 => {
            expanded = hex.chars().flat_map(|ch| [ch, ch]).collect::<String>();
            expanded.as_str()
        }
        6 | 8 => hex,
        _ => return None,
    };
    u32::from_str_radix(&hex[..6], 16).ok()
}

fn group_label(i18n: &I18n, group: Option<&str>) -> String {
    group
        .filter(|group| !group.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| i18n.t("sessionManager.folder_tree.ungrouped"))
}

fn selected_count_label(i18n: &I18n, count: usize) -> String {
    i18n.t("sessionManager.table.selected_count")
        .replace("{{count}}", &count.to_string())
}

fn connections_deleted_label(i18n: &I18n, count: usize) -> String {
    i18n.t("sessionManager.toast.connections_deleted")
        .replace("{{count}}", &count.to_string())
}

fn connections_moved_label(i18n: &I18n, count: usize, group: String) -> String {
    i18n.t("sessionManager.toast.connections_moved")
        .replace("{{count}}", &count.to_string())
        .replace("{{group}}", &group)
}

fn current_username() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "root".to_string())
}

pub(super) fn saved_connection_from_ssh_host(
    host: SshConfigHost,
) -> anyhow::Result<SavedConnection> {
    let now = chrono::Utc::now();
    let auth = match (host.identity_file, host.certificate_file) {
        (Some(key_path), Some(cert_path)) => SavedAuth::Certificate {
            key_path,
            cert_path,
            has_passphrase: false,
            passphrase_keychain_id: None,
            plaintext_passphrase: None,
        },
        (Some(key_path), None) => SavedAuth::Key {
            key_path,
            has_passphrase: false,
            passphrase_keychain_id: None,
            plaintext_passphrase: None,
        },
        _ => SavedAuth::Agent,
    };
    Ok(SavedConnection {
        id: String::new(),
        name: host.alias.clone(),
        group: Some(IMPORTED_GROUP.to_string()),
        host: host.hostname.unwrap_or(host.alias),
        port: host.port.unwrap_or(22),
        username: host.user.unwrap_or_else(current_username),
        auth,
        options: oxideterm_connections::ConnectionOptions::default(),
        created_at: now,
        last_used_at: None,
        updated_at: Some(now),
        color: None,
        tags: vec![SSH_CONFIG_TAG.to_string()],
    })
}

pub(super) fn saved_auth_from_form(form: &NewConnectionForm) -> SavedAuth {
    match form.auth_tab {
        SshAuthTab::Password => SavedAuth::Password {
            keychain_id: None,
            plaintext_password: form.save_password.then(|| form.password.clone()),
        },
        SshAuthTab::DefaultKey => SavedAuth::Key {
            key_path: String::new(),
            has_passphrase: !form.passphrase.is_empty(),
            passphrase_keychain_id: None,
            plaintext_passphrase: (!form.passphrase.is_empty()).then(|| form.passphrase.clone()),
        },
        SshAuthTab::SshKey => SavedAuth::Key {
            key_path: form.key_path.trim().to_string(),
            has_passphrase: !form.passphrase.is_empty(),
            passphrase_keychain_id: None,
            plaintext_passphrase: (!form.passphrase.is_empty()).then(|| form.passphrase.clone()),
        },
        SshAuthTab::Certificate => SavedAuth::Certificate {
            key_path: form.key_path.trim().to_string(),
            cert_path: form.cert_path.trim().to_string(),
            has_passphrase: !form.passphrase.is_empty(),
            passphrase_keychain_id: None,
            plaintext_passphrase: (!form.passphrase.is_empty()).then(|| form.passphrase.clone()),
        },
        SshAuthTab::Agent | SshAuthTab::TwoFactor => SavedAuth::Agent,
    }
}

fn saved_auth_from_form_for_update(
    form: &NewConnectionForm,
    existing_auth: Option<&SavedAuth>,
) -> SavedAuth {
    if form.auth_tab == SshAuthTab::Password && !form.password_loaded {
        if let Some(SavedAuth::Password {
            keychain_id,
            plaintext_password,
        }) = existing_auth
        {
            return SavedAuth::Password {
                keychain_id: keychain_id.clone(),
                plaintext_password: plaintext_password.clone(),
            };
        }
        return SavedAuth::Password {
            keychain_id: None,
            plaintext_password: None,
        };
    }

    if form.auth_tab == SshAuthTab::Password {
        return SavedAuth::Password {
            keychain_id: form.saved_password_keychain_id.clone(),
            plaintext_password: Some(form.password.clone()),
        };
    }

    saved_auth_from_form(form)
}

pub(super) fn form_from_saved_connection(
    conn: &SavedConnection,
    error: Option<String>,
) -> NewConnectionForm {
    let (auth_tab, password, key_path, cert_path, passphrase, save_password) = match &conn.auth {
        SavedAuth::Password {
            keychain_id,
            plaintext_password,
        } => (
            SshAuthTab::Password,
            plaintext_password.clone().unwrap_or_default(),
            String::new(),
            String::new(),
            String::new(),
            keychain_id.is_some() || plaintext_password.is_some(),
        ),
        SavedAuth::Key {
            key_path,
            has_passphrase,
            passphrase_keychain_id,
            plaintext_passphrase,
        } if key_path.is_empty() => (
            SshAuthTab::DefaultKey,
            String::new(),
            key_path.clone(),
            String::new(),
            String::new(),
            *has_passphrase || passphrase_keychain_id.is_some() || plaintext_passphrase.is_some(),
        ),
        SavedAuth::Key {
            key_path,
            has_passphrase,
            passphrase_keychain_id,
            plaintext_passphrase,
        } => (
            SshAuthTab::SshKey,
            String::new(),
            key_path.clone(),
            String::new(),
            String::new(),
            *has_passphrase || passphrase_keychain_id.is_some() || plaintext_passphrase.is_some(),
        ),
        SavedAuth::Certificate {
            key_path,
            cert_path,
            has_passphrase,
            passphrase_keychain_id,
            plaintext_passphrase,
        } => (
            SshAuthTab::Certificate,
            String::new(),
            key_path.clone(),
            cert_path.clone(),
            String::new(),
            *has_passphrase || passphrase_keychain_id.is_some() || plaintext_passphrase.is_some(),
        ),
        SavedAuth::Agent => (
            SshAuthTab::Agent,
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            false,
        ),
    };
    NewConnectionForm {
        name: conn.name.clone(),
        host: conn.host.clone(),
        port: conn.port.to_string(),
        username: conn.username.clone(),
        auth_tab,
        password,
        saved_password_keychain_id: match &conn.auth {
            SavedAuth::Password { keychain_id, .. } => keychain_id.clone(),
            _ => None,
        },
        password_loaded: false,
        password_visible: false,
        password_loading: false,
        password_error: None,
        key_path,
        cert_path,
        passphrase,
        save_password,
        group: group_label_for_form(conn.group.as_deref()),
        color: conn.color.clone().unwrap_or_default(),
        tags: conn.tags.clone(),
        agent_forwarding: conn.options.agent_forwarding,
        save_connection: true,
        error,
        ..NewConnectionForm::default()
    }
}

pub(super) fn save_request_from_form(
    form: &NewConnectionForm,
    id: Option<String>,
) -> anyhow::Result<SaveConnectionRequest> {
    save_request_from_form_with_existing_auth(form, id, None)
}

pub(super) fn save_request_from_form_with_existing_auth(
    form: &NewConnectionForm,
    id: Option<String>,
    existing_auth: Option<&SavedAuth>,
) -> anyhow::Result<SaveConnectionRequest> {
    let port = form.port.trim().parse::<u16>().unwrap_or(22);
    Ok(SaveConnectionRequest {
        id,
        name: form.name.trim().to_string(),
        group: Some(form.group.trim().to_string()),
        host: form.host.trim().to_string(),
        port,
        username: form.username.trim().to_string(),
        auth: if existing_auth.is_some() {
            saved_auth_from_form_for_update(form, existing_auth)
        } else {
            saved_auth_from_form(form)
        },
        color: (!form.color.trim().is_empty()).then(|| form.color.trim().to_string()),
        tags: form.tags.clone(),
        agent_forwarding: form.agent_forwarding,
    })
}

pub(super) fn ssh_config_from_saved_connection(
    conn: &SavedConnection,
    loaded_password: Option<String>,
    loaded_passphrase: Option<String>,
) -> Option<SshConfig> {
    let auth = match &conn.auth {
        SavedAuth::Password {
            plaintext_password: Some(password),
            ..
        } => AuthMethod::password(password.clone()),
        SavedAuth::Password {
            keychain_id: Some(_),
            ..
        } => AuthMethod::password(loaded_password?),
        SavedAuth::Password {
            keychain_id: None,
            plaintext_password: None,
        } => return None,
        SavedAuth::Key {
            key_path,
            plaintext_passphrase,
            ..
        } => AuthMethod::key(
            key_path.clone(),
            plaintext_passphrase
                .clone()
                .or_else(|| loaded_passphrase.clone()),
        ),
        SavedAuth::Certificate {
            key_path,
            cert_path,
            plaintext_passphrase,
            ..
        } => AuthMethod::certificate(
            key_path.clone(),
            cert_path.clone(),
            plaintext_passphrase
                .clone()
                .or_else(|| loaded_passphrase.clone()),
        ),
        SavedAuth::Agent => AuthMethod::Agent,
    };
    Some(SshConfig {
        host: conn.host.clone(),
        port: conn.port,
        username: conn.username.clone(),
        auth,
        agent_forwarding: conn.options.agent_forwarding,
        strict_host_key_checking: true,
        ..SshConfig::default()
    })
}

fn group_label_for_form(group: Option<&str>) -> String {
    group.unwrap_or_default().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_form() -> NewConnectionForm {
        NewConnectionForm {
            name: "Home".to_string(),
            host: "192.168.1.2".to_string(),
            port: "22".to_string(),
            username: "me".to_string(),
            group: "Ungrouped".to_string(),
            ..NewConnectionForm::default()
        }
    }

    #[test]
    fn new_connection_save_password_false_does_not_request_keychain_storage() {
        let form = NewConnectionForm {
            password: "secret".to_string(),
            save_password: false,
            ..base_form()
        };

        let request = save_request_from_form(&form, None).unwrap();

        match request.auth {
            SavedAuth::Password {
                keychain_id: None,
                plaintext_password: None,
            } => {}
            other => panic!("unexpected auth: {other:?}"),
        }
    }

    #[test]
    fn new_connection_save_password_true_keeps_empty_password_as_submitted_secret() {
        let form = NewConnectionForm {
            password: String::new(),
            save_password: true,
            ..base_form()
        };

        let request = save_request_from_form(&form, None).unwrap();

        match request.auth {
            SavedAuth::Password {
                keychain_id: None,
                plaintext_password: Some(password),
            } => assert_eq!(password, ""),
            other => panic!("unexpected auth: {other:?}"),
        }
    }

    #[test]
    fn edit_properties_unloaded_password_preserves_saved_keychain_id() {
        let existing = SavedAuth::Password {
            keychain_id: Some("kc-password".to_string()),
            plaintext_password: None,
        };
        let form = NewConnectionForm {
            password: String::new(),
            password_loaded: false,
            save_password: true,
            ..base_form()
        };

        let request = save_request_from_form_with_existing_auth(
            &form,
            Some("conn-1".to_string()),
            Some(&existing),
        )
        .unwrap();

        match request.auth {
            SavedAuth::Password {
                keychain_id: Some(keychain_id),
                plaintext_password: None,
            } => assert_eq!(keychain_id, "kc-password"),
            other => panic!("unexpected auth: {other:?}"),
        }
    }

    #[test]
    fn edit_properties_same_key_empty_passphrase_submits_no_new_secret() {
        let existing = SavedAuth::Key {
            key_path: "/tmp/id_ed25519".to_string(),
            has_passphrase: true,
            passphrase_keychain_id: Some("kc-passphrase".to_string()),
            plaintext_passphrase: None,
        };
        let form = NewConnectionForm {
            auth_tab: SshAuthTab::SshKey,
            key_path: "/tmp/id_ed25519".to_string(),
            passphrase: String::new(),
            ..base_form()
        };

        let request = save_request_from_form_with_existing_auth(
            &form,
            Some("conn-1".to_string()),
            Some(&existing),
        )
        .unwrap();

        match request.auth {
            SavedAuth::Key {
                key_path,
                has_passphrase,
                passphrase_keychain_id: None,
                plaintext_passphrase: None,
            } => {
                assert_eq!(key_path, "/tmp/id_ed25519");
                assert!(!has_passphrase);
            }
            other => panic!("unexpected auth: {other:?}"),
        }
    }
}
