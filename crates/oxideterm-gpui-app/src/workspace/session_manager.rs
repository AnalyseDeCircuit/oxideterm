use std::collections::HashSet;

use gpui::{StatefulInteractiveElement, prelude::*};
use oxideterm_connections::{
    AuthType, ConnectionInfo, SaveConnectionRequest, SavedAuth, SavedConnection, SshConfigHost,
    list_ssh_config_hosts, resolve_ssh_config_alias,
};
use oxideterm_gpui_ui::{
    button::{ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, button_with},
    checkbox, text_input_anchor_probe,
};
use oxideterm_ssh::AuthMethod;

use super::*;
use crate::workspace::ime::WorkspaceImeTarget;

const UNGROUPED_FILTER: &str = "__ungrouped__";
const RECENT_FILTER: &str = "__recent__";
const IMPORTED_GROUP: &str = "Imported";
const SSH_CONFIG_TAG: &str = "ssh-config";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(super) enum SessionManagerInput {
    Search,
    NewGroup,
}

impl SessionManagerInput {
    pub(super) fn anchor_key(self) -> u64 {
        match self {
            Self::Search => 1,
            Self::NewGroup => 2,
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

#[derive(Clone, Debug)]
pub(super) struct SessionManagerState {
    pub(super) selected_group: Option<String>,
    pub(super) search_query: String,
    pub(super) sort_field: SessionSortField,
    pub(super) sort_direction: SortDirection,
    pub(super) selected_ids: HashSet<String>,
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
            sort_field: SessionSortField::LastUsed,
            sort_direction: SortDirection::Desc,
            selected_ids: HashSet::new(),
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
    pub(super) fn render_session_manager_surface(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.bg))
            .text_color(rgb(theme.text))
            .child(self.render_session_manager_toolbar(cx))
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .flex()
                    .flex_row()
                    .child(self.render_session_manager_folder_tree(cx))
                    .child(self.render_session_manager_table(cx)),
            )
            .when_some(self.session_manager.status.clone(), |surface, status| {
                surface.child(
                    div()
                        .h(px(32.0))
                        .flex()
                        .items_center()
                        .px_4()
                        .border_t_1()
                        .border_color(rgb(theme.border))
                        .bg(rgb(theme.bg_panel))
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
                    SessionManagerInput::NewGroup => self.session_manager.new_group_name.pop(),
                };
                self.clear_session_selection_for_invisible_rows();
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
        window.focus(&self.focus_handle);
        self.reveal_active_tab(window);
        self.persist_sidebar_settings();
        cx.notify();
    }

    fn render_session_manager_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let selected_count = self.session_manager.selected_ids.len();
        div()
            .h(px(50.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .px_4()
            .border_b_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg_panel))
            .child(div().w(px(280.0)).child(self.render_session_text_input(
                SessionManagerInput::Search,
                &self.session_manager.search_query,
                self.i18n.t("sessionManager.toolbar.search_placeholder"),
                cx,
            )))
            .child(
                self.render_session_manager_button(
                    LucideIcon::Plus,
                    self.i18n.t("sessionManager.toolbar.new_connection"),
                    ButtonVariant::Default,
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
                self.render_session_manager_button(
                    LucideIcon::Folder,
                    self.i18n.t("sessionManager.folder_tree.new_group"),
                    ButtonVariant::Secondary,
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
                ),
            )
            .child(
                self.render_session_manager_button(
                    LucideIcon::FolderInput,
                    self.i18n.t("sessionManager.toolbar.import"),
                    ButtonVariant::Secondary,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.open_ssh_config_import(cx);
                        cx.stop_propagation();
                    }),
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
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .text_color(rgb(theme.text_muted))
                                    .child(selected_count_label(&self.i18n, selected_count)),
                            )
                            .child(
                                self.render_session_manager_button(
                                    LucideIcon::FolderOpen,
                                    self.i18n.t("sessionManager.batch.move_to_group"),
                                    ButtonVariant::Secondary,
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
                                    ButtonVariant::Destructive,
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
            .into_any_element()
    }

    fn render_session_manager_folder_tree(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let all_count = self.connection_store.connections().len();
        let ungrouped_count = self
            .connection_store
            .connections()
            .iter()
            .filter(|conn| conn.group.is_none())
            .count();
        let recent_count = self
            .connection_store
            .connections()
            .iter()
            .filter(|conn| conn.last_used_at.is_some())
            .count()
            .min(20);
        let mut tree = div()
            .id("session-manager-folder-tree")
            .w(px(220.0))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg_panel))
            .overflow_y_scroll()
            .py_2()
            .child(self.render_group_tree_item(
                None,
                LucideIcon::LayoutList,
                self.i18n.t("sessionManager.folder_tree.all_connections"),
                all_count,
                0,
                cx,
            ));

        for group in self.connection_store.groups() {
            tree = tree.child(self.render_group_tree_item(
                Some(group.clone()),
                LucideIcon::Folder,
                group.rsplit('/').next().unwrap_or(group).to_string(),
                self.connection_count_for_group(group),
                group.matches('/').count(),
                cx,
            ));
        }

        if ungrouped_count > 0 {
            tree = tree.child(self.render_group_tree_item(
                Some(UNGROUPED_FILTER.to_string()),
                LucideIcon::Server,
                self.i18n.t("sessionManager.folder_tree.ungrouped"),
                ungrouped_count,
                0,
                cx,
            ));
        }

        tree.child(
            div()
                .mt_2()
                .pt_2()
                .border_t_1()
                .border_color(rgb(theme.border))
                .child(self.render_group_tree_item(
                    Some(RECENT_FILTER.to_string()),
                    LucideIcon::Activity,
                    self.i18n.t("sessionManager.folder_tree.recent"),
                    recent_count,
                    0,
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
        count: usize,
        depth: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active = self.session_manager.selected_group == group;
        div()
            .h(px(34.0))
            .mx_2()
            .pl(px(10.0 + depth as f32 * 14.0))
            .pr_2()
            .flex()
            .items_center()
            .gap(px(8.0))
            .rounded(px(self.tokens.radii.md))
            .cursor_pointer()
            .bg(if active {
                rgb(theme.bg_active)
            } else {
                rgb(theme.bg_panel)
            })
            .text_color(if active {
                rgb(theme.accent)
            } else {
                rgb(theme.text)
            })
            .hover(move |item| {
                if active {
                    item
                } else {
                    item.bg(rgb(theme.bg_hover))
                }
            })
            .child(Self::render_lucide_icon(
                icon,
                15.0,
                if active {
                    rgb(theme.accent)
                } else {
                    rgb(theme.text_muted)
                },
            ))
            .child(
                div()
                    .flex_1()
                    .truncate()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(if active {
                        gpui::FontWeight::SEMIBOLD
                    } else {
                        gpui::FontWeight::NORMAL
                    })
                    .child(label),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(count.to_string()),
            )
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

    fn render_session_manager_table(&self, cx: &mut Context<Self>) -> AnyElement {
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
            .bg(rgb(theme.bg))
            .child(self.render_connection_table_header(all_selected, cx))
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
                                .gap(px(8.0))
                                .text_color(rgb(theme.text_muted))
                                .child(Self::render_lucide_icon(
                                    LucideIcon::Server,
                                    42.0,
                                    rgba((theme.text_muted << 8) | 0x66),
                                ))
                                .child(
                                    div()
                                        .text_size(px(15.0))
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .child(empty_message),
                                )
                                .child(div().text_size(px(self.tokens.metrics.ui_text_sm)).child(
                                    self.i18n.t("sessionManager.table.no_connections_hint"),
                                )),
                        )
                    })
                    .children(
                        rows.into_iter()
                            .map(|conn| self.render_connection_table_row(conn, cx)),
                    ),
            )
            .into_any_element()
    }

    fn render_connection_table_header(
        &self,
        all_selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .h(px(38.0))
            .flex()
            .items_center()
            .border_b_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg_panel))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(rgb(theme.text_muted))
            .child(
                div()
                    .w(px(44.0))
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
                190.0,
                cx,
            ))
            .child(self.render_sort_header(
                "sessionManager.table.host",
                SessionSortField::Host,
                180.0,
                cx,
            ))
            .child(self.render_sort_header(
                "sessionManager.table.port",
                SessionSortField::Port,
                70.0,
                cx,
            ))
            .child(self.render_sort_header(
                "sessionManager.table.username",
                SessionSortField::Username,
                130.0,
                cx,
            ))
            .child(self.render_sort_header(
                "sessionManager.table.auth_type",
                SessionSortField::AuthType,
                100.0,
                cx,
            ))
            .child(self.render_sort_header(
                "sessionManager.table.group",
                SessionSortField::Group,
                140.0,
                cx,
            ))
            .child(self.render_sort_header(
                "sessionManager.table.last_used",
                SessionSortField::LastUsed,
                120.0,
                cx,
            ))
            .child(
                div()
                    .w(px(210.0))
                    .px_2()
                    .child(self.i18n.t("sessionManager.table.actions")),
            )
            .into_any_element()
    }

    fn render_sort_header(
        &self,
        label_key: &'static str,
        field: SessionSortField,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = self.session_manager.sort_field == field;
        let suffix = if active {
            match self.session_manager.sort_direction {
                SortDirection::Asc => " ↑",
                SortDirection::Desc => " ↓",
            }
        } else {
            ""
        };
        div()
            .w(px(width))
            .px_2()
            .cursor_pointer()
            .child(format!("{}{}", self.i18n.t(label_key), suffix))
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
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let selected = self.session_manager.selected_ids.contains(&conn.id);
        let id = conn.id.clone();
        div()
            .h(px(46.0))
            .flex()
            .items_center()
            .border_b_1()
            .border_color(rgba((theme.border << 8) | 0x80))
            .bg(if selected {
                rgba((theme.bg_active << 8) | 0xcc)
            } else {
                rgb(theme.bg)
            })
            .hover(move |row| row.bg(rgb(theme.bg_hover)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    this.open_saved_connection(&id, window, cx);
                    cx.stop_propagation();
                }),
            )
            .child(
                div()
                    .w(px(44.0))
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
            .child(self.render_table_cell(conn.name.clone(), 190.0, true))
            .child(self.render_table_cell(conn.host.clone(), 180.0, false))
            .child(self.render_table_cell(conn.port.to_string(), 70.0, false))
            .child(self.render_table_cell(conn.username.clone(), 130.0, false))
            .child(self.render_table_cell(auth_label(conn.auth_type), 100.0, false))
            .child(self.render_table_cell(
                group_label(&self.i18n, conn.group.as_deref()),
                140.0,
                false,
            ))
            .child(
                self.render_table_cell(
                    conn.last_used_at
                        .clone()
                        .unwrap_or_else(|| self.i18n.t("sessionManager.table.never_used")),
                    120.0,
                    false,
                ),
            )
            .child(self.render_row_actions(conn, cx))
            .into_any_element()
    }

    fn render_table_cell(&self, text: String, width: f32, strong: bool) -> AnyElement {
        div()
            .w(px(width))
            .px_2()
            .truncate()
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .font_weight(if strong {
                gpui::FontWeight::SEMIBOLD
            } else {
                gpui::FontWeight::NORMAL
            })
            .child(text)
            .into_any_element()
    }

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

    fn render_session_text_input(
        &self,
        target: SessionManagerInput,
        value: &str,
        placeholder: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let workspace = cx.entity();
        let active = self.session_manager.focused_input == Some(target);
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
                .px_2()
                .flex()
                .items_center()
                .rounded(px(self.tokens.radii.md))
                .border_1()
                .border_color(if active {
                    rgb(theme.accent)
                } else {
                    rgb(theme.border)
                })
                .bg(rgb(theme.bg))
                .text_size(px(self.tokens.metrics.ui_text_sm))
                .text_color(if value.is_empty() {
                    rgb(theme.text_muted)
                } else {
                    rgb(theme.text)
                })
                .child(text)
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

fn saved_connection_from_ssh_host(host: SshConfigHost) -> anyhow::Result<SavedConnection> {
    let now = chrono::Utc::now();
    let auth = match (host.identity_file, host.certificate_file) {
        (Some(key_path), Some(cert_path)) => SavedAuth::Certificate {
            key_path,
            cert_path,
            passphrase: None,
        },
        (Some(key_path), None) => SavedAuth::Key {
            key_path,
            passphrase: None,
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
            password: (form.save_password && !form.password.is_empty())
                .then(|| form.password.clone()),
        },
        SshAuthTab::DefaultKey => SavedAuth::Key {
            key_path: String::new(),
            passphrase: (!form.passphrase.is_empty()).then(|| form.passphrase.clone()),
        },
        SshAuthTab::SshKey => SavedAuth::Key {
            key_path: form.key_path.trim().to_string(),
            passphrase: (!form.passphrase.is_empty()).then(|| form.passphrase.clone()),
        },
        SshAuthTab::Certificate => SavedAuth::Certificate {
            key_path: form.key_path.trim().to_string(),
            cert_path: form.cert_path.trim().to_string(),
            passphrase: (!form.passphrase.is_empty()).then(|| form.passphrase.clone()),
        },
        SshAuthTab::Agent | SshAuthTab::TwoFactor => SavedAuth::Agent,
    }
}

pub(super) fn form_from_saved_connection(
    conn: &SavedConnection,
    error: Option<String>,
) -> NewConnectionForm {
    let (auth_tab, password, key_path, cert_path, passphrase, save_password) = match &conn.auth {
        SavedAuth::Password { password } => (
            SshAuthTab::Password,
            password.clone().unwrap_or_default(),
            String::new(),
            String::new(),
            String::new(),
            password.is_some(),
        ),
        SavedAuth::Key {
            key_path,
            passphrase,
        } if key_path.is_empty() => (
            SshAuthTab::DefaultKey,
            String::new(),
            key_path.clone(),
            String::new(),
            passphrase.clone().unwrap_or_default(),
            false,
        ),
        SavedAuth::Key {
            key_path,
            passphrase,
        } => (
            SshAuthTab::SshKey,
            String::new(),
            key_path.clone(),
            String::new(),
            passphrase.clone().unwrap_or_default(),
            false,
        ),
        SavedAuth::Certificate {
            key_path,
            cert_path,
            passphrase,
        } => (
            SshAuthTab::Certificate,
            String::new(),
            key_path.clone(),
            cert_path.clone(),
            passphrase.clone().unwrap_or_default(),
            false,
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
        key_path,
        cert_path,
        passphrase,
        save_password,
        group: group_label_for_form(conn.group.as_deref()),
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
    let port = form.port.trim().parse::<u16>().unwrap_or(22);
    Ok(SaveConnectionRequest {
        id,
        name: form.name.trim().to_string(),
        group: Some(form.group.trim().to_string()),
        host: form.host.trim().to_string(),
        port,
        username: form.username.trim().to_string(),
        auth: saved_auth_from_form(form),
        color: None,
        tags: Vec::new(),
        agent_forwarding: form.agent_forwarding,
    })
}

pub(super) fn ssh_config_from_saved_connection(conn: &SavedConnection) -> Option<SshConfig> {
    let auth = match &conn.auth {
        SavedAuth::Password { password } => AuthMethod::password(password.clone()?),
        SavedAuth::Key {
            key_path,
            passphrase,
        } => AuthMethod::key(key_path.clone(), passphrase.clone()),
        SavedAuth::Certificate {
            key_path,
            cert_path,
            passphrase,
        } => AuthMethod::certificate(key_path.clone(), cert_path.clone(), passphrase.clone()),
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
